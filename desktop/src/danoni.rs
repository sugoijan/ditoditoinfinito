//! Opt-in desktop URL jobs. Installed runtime integrity and this exact host
//! executable/lock binding are independent gates; neither authenticates a vendor.
//! No runtime is bundled or automatically downloaded by this development feature.
use ddi_danoni_export::{
    assets::sha256,
    import_host, import_job,
    managed_store::Store,
    native,
    network::NetworkCancel,
    runtime::{HostConfig, Installation},
};
use ddi_ui::screens::data::{ImportReportView, ImportView};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
};

/// Host-selected configuration, never obtained from the URL. The binding is
/// deliberately external to the executable (so it can name the final hash).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    host: PathBuf,
    application_sha256: String,
    application_lock_sha256: String,
}
const LOCK: &[u8] = include_bytes!("../../Cargo.lock");
const SETUP: &str = "Set DDI_DANONI_HOST to a desktop binding JSON (see docs/plans/danoni-import.md). This development feature needs a matching installed runtime; it does not install one.";

fn config() -> Result<HostConfig, String> {
    let path = std::env::var_os("DDI_DANONI_HOST")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| SETUP.to_string())?;
    let raw =
        std::fs::read(&path).map_err(|e| format!("Cannot read desktop binding: {e}. {SETUP}"))?;
    if raw.len() > 64 * 1024 {
        return Err("Desktop binding exceeds 64 KiB".into());
    }
    let c: Config =
        serde_json::from_slice(&raw).map_err(|e| format!("Invalid desktop binding: {e}"))?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    // The operator binds a reviewed final application, not the standalone SDK
    // lock's attestation. No runtime inventory or worker checks are bypassed.
    let bytes = std::fs::read(executable).map_err(|e| e.to_string())?;
    check_binding(&c, &bytes, LOCK)?;
    let host = if c.host.is_absolute() {
        c.host
    } else {
        path.parent().unwrap_or(Path::new(".")).join(c.host)
    };
    HostConfig::read(&host).map_err(|e| format!("Cannot read host configuration: {e}"))
}
fn check_binding(c: &Config, executable: &[u8], lock: &[u8]) -> Result<(), String> {
    if c.application_sha256 != sha256(executable) || c.application_lock_sha256 != sha256(lock) {
        return Err("Desktop application/lock binding differs. Review this build and create a fresh binding; a standalone SDK runtime does not attest the game binary.".into());
    }
    Ok(())
}

pub struct News {
    pub restore: Vec<String>,
    pub songs: Option<Vec<crate::library::LibSong>>,
    pub report: Option<ImportReportView>,
}
struct Finished {
    restore: Vec<String>,
    songs: Option<Vec<crate::library::LibSong>>,
    report: Option<ImportReportView>,
}
struct Job {
    cancel: NetworkCancel,
    stage: Arc<Mutex<Option<String>>>,
    done: mpsc::Receiver<Finished>,
    thread: JoinHandle<()>,
}
/// One owned worker. Progress is coalesced; the separate terminal slot cannot be
/// overwritten by progress. The event loop drains on exit and never joins live work.
#[derive(Default)]
pub struct Imports {
    job: Option<Job>,
    report: Option<ImportReportView>,
    shutting_down: bool,
    sent: Option<ImportView>,
}
impl Imports {
    pub fn view(&self, view: &mut ImportView) {
        view.danoni_urls = true;
        view.danoni_cancel = self.job.is_some() && !self.shutting_down;
        view.running = self.job.as_ref().map(|j| {
            j.stage
                .lock()
                .ok()
                .and_then(|s| s.clone())
                .unwrap_or_else(|| "Preparing DanOni import…".into())
        });
        view.report = self.report.clone();
    }
    pub fn changed_view(&mut self) -> Option<ImportView> {
        let mut view = ImportView {
            platform: ddi_ui::screens::data::Platform::Desktop,
            ..Default::default()
        };
        self.view(&mut view);
        if self.sent.as_ref() == Some(&view) {
            return None;
        }
        self.sent = Some(view.clone());
        Some(view)
    }
    pub fn start(&mut self, url: String, wake: Arc<dyn Fn() + Send + Sync>) {
        if self.job.is_some() || self.shutting_down {
            return;
        }
        self.spawn(Some(url), wake);
    }
    /// Initial catalog load also runs away from the UI thread. A missing host
    /// setting is quiet until the user asks to import.
    pub fn load(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        if std::env::var_os("DDI_DANONI_HOST").is_some() && self.job.is_none() {
            self.spawn(None, wake);
        }
    }
    fn spawn(&mut self, url: Option<String>, wake: Arc<dyn Fn() + Send + Sync>) {
        self.report = None;
        let cancel = NetworkCancel::default();
        let stage = Arc::new(Mutex::new(Some("Opening DanOni catalog…".to_string())));
        let (tx, done) = mpsc::channel();
        let token = cancel.clone();
        let progress = stage.clone();
        match std::thread::Builder::new().name("ddi-danoni".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(url.as_deref(), &token, &progress, &*wake)));
            let finished = match result {
                Ok(Ok(f)) => f,
                Ok(Err(error)) => Finished { restore: Vec::new(), songs: None, report: Some(report(error, true)) },
                Err(_) => Finished { restore: Vec::new(), songs: None, report: Some(report("DanOni worker stopped unexpectedly. Reopen the app to verify/recover its catalog.".into(), true)) },
            };
            let _ = tx.send(finished); wake();
        }) {
            Ok(thread) => self.job = Some(Job {cancel, stage, done, thread}),
            Err(e) => self.report = Some(report(format!("Cannot start DanOni import: {e}"), true)),
        }
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn cancel(&self) {
        if let Some(j) = &self.job {
            j.cancel.cancel();
        }
    }
    pub fn shutdown(&mut self) -> bool {
        self.shutting_down = true;
        self.cancel();
        self.job.is_none()
    }
    pub fn drained(&self) -> bool {
        self.shutting_down && self.job.is_none()
    }
    pub fn poll(&mut self) -> Option<News> {
        if !self.job.as_ref()?.thread.is_finished() {
            return None;
        }
        let job = self.job.take()?;
        let result = job.done.try_recv().unwrap_or_else(|_| Finished {
            restore: Vec::new(),
            songs: None,
            report: Some(report(
                "DanOni worker lost its completion result".into(),
                true,
            )),
        });
        let _ = job.thread.join();
        self.report = result.report.clone();
        Some(News {
            restore: result.restore,
            songs: result.songs,
            report: result.report,
        })
    }
}
impl Drop for Imports {
    fn drop(&mut self) {
        self.cancel();
        // Normal application exit drains through the event loop. On abnormal
        // host teardown transfer ownership to a reaper; don't block a UI drop.
        if let Some(job) = self.job.take() {
            let _ = std::thread::Builder::new()
                .name("ddi-danoni-drain".into())
                .spawn(move || {
                    let _ = job.thread.join();
                });
        }
    }
}
fn report(summary: String, error: bool) -> ImportReportView {
    if error {
        ImportReportView {
            summary: "DanOni import could not finish.".into(),
            error,
            sections: vec![("Details".into(), vec![summary])],
        }
    } else {
        ImportReportView {
            summary,
            error,
            sections: Vec::new(),
        }
    }
}
fn run(
    url: Option<&str>,
    cancel: &NetworkCancel,
    stage: &Mutex<Option<String>>,
    wake: &dyn Fn(),
) -> Result<Finished, String> {
    run_config(config()?, url, cancel, stage, wake)
}
fn run_config(
    config: HostConfig,
    url: Option<&str>,
    cancel: &NetworkCancel,
    stage: &Mutex<Option<String>>,
    wake: &dyn Fn(),
) -> Result<Finished, String> {
    if let Some(url) = url {
        let parsed = url::Url::parse(url)
            .map_err(|_| "Enter a complete http:// or https:// DanOni work URL.")?;
        if url.len() > 8192
            || !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err("Enter a complete http:// or https:// DanOni work URL without embedded credentials.".into());
        }
    }
    // Verification precedes any network acquisition; catalog loading itself does
    // not need a worker (already published songs remain playable).
    let installation = url.map(|_| Installation::locate(config.runtime.as_deref())).transpose().map_err(|e| format!("DanOni runtime unavailable: {e}. Reinstall a matching runtime; see docs/plans/danoni-import.md."))?;
    let mut store = match std::fs::symlink_metadata(&config.library) {
        Ok(_) => Store::open(&config.library),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && url.is_none() => return Ok(Finished {restore: Vec::new(), songs: Some(Vec::new()), report: None}),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::create(&config.library),
        Err(e) => return Err(format!("Cannot inspect DanOni catalog: {e}")),
    }.map_err(|e| format!("Cannot open DanOni catalog: {e}. Ensure the configured library parent is an existing private folder outside Git."))?;
    store
        .recover()
        .map_err(|e| format!("DanOni catalog needs recovery: {e}"))?;
    let mut terminal = None;
    let mut restore = Vec::new();
    if let (Some(url), Some(install)) = (url, installation) {
        let output = config.staging.join(format!(
            "desktop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        let assets = native::DirectorySource::new(BTreeMap::new()).map_err(|e| e.to_string())?;
        let result = import_job::execute(
            &install.request(url, config.offline),
            &import_job::Services {
                cache: &config.cache,
                worker: &install.worker,
                assets: &assets,
            },
            &output,
            &mut store,
            cancel,
            &mut |url| {
                let origin = url::Url::parse(url)
                    .map_err(|e| ddi_danoni_export::Error {
                        code: "invalid-url",
                        message: e.to_string(),
                    })?
                    .origin()
                    .ascii_serialization();
                let decision = config
                    .origins
                    .get(&origin)
                    .cloned()
                    .filter(|s| !s.trim().is_empty());
                decision.ok_or_else(|| ddi_danoni_export::Error { code: "site-terms-denied", message: format!("Review public-access terms for {origin} and configure that exact origin decision in host.json; robots rules still apply.") })
            },
            &mut |p| {
                let label = match p {
                    import_job::Progress::Preparing { .. } => {
                        "Downloading and extracting the work…"
                    }
                    import_job::Progress::Validating { .. } => "Validating prepared songs…",
                    import_job::Progress::Planned { .. } => "Checking existing imports…",
                    import_job::Progress::Installing { .. } => "Publishing songs…",
                };
                let changed = stage.lock().is_ok_and(|mut s| {
                    if s.as_deref() == Some(label) {
                        return false;
                    }
                    *s = Some(label.into());
                    true
                });
                // Coalesce wakeups as well as payloads; repeated chunk progress
                // must not flood the event-loop proxy's queue.
                if changed {
                    wake();
                }
                Ok(())
            },
        );
        terminal = Some(match result {
            Ok(outcome) => {
                if !matches!(
                    &outcome.installation,
                    import_host::Outcome::NeedsRecovery { .. }
                ) {
                    restore = outcome
                        .review
                        .plan
                        .operations
                        .iter()
                        .map(|op| op.id.clone())
                        .collect();
                }
                match outcome.installation {
                    import_host::Outcome::Installed {
                        added,
                        replaced,
                        skipped,
                        ..
                    } => report(
                        format!(
                            "DanOni import complete: {added} added, {replaced} replaced, {skipped} skipped."
                        ),
                        false,
                    ),
                    import_host::Outcome::Unchanged { .. } => report(
                        "These songs are already in your DanOni library.".into(),
                        false,
                    ),
                    import_host::Outcome::NeedsRecovery { reason, .. } => report(
                        format!(
                            "Import requires catalog recovery: {reason}. Restart before retrying."
                        ),
                        true,
                    ),
                }
            }
            Err(e) if e.code == "request-cancelled" => report(
                "DanOni import cancelled before publication. No new songs were added.".into(),
                false,
            ),
            Err(e) => report(format!("DanOni import stopped: {e}"), true),
        });
    }
    // Even a post-commit error cannot hide published entries. Never load staging.
    let songs = store
        .playback_snapshot()
        .map_err(|e| format!("Cannot verify DanOni catalog: {e}"))?;
    let songs = crate::library::managed::songs(songs)?;
    Ok(Finished {
        restore,
        songs: Some(songs),
        report: terminal,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_drains_and_preserves_committed_terminal_result() {
        let cancel = NetworkCancel::default();
        let token = cancel.clone();
        let (tx, done) = mpsc::channel();
        let (ready_tx, ready) = mpsc::channel();
        let stage = Arc::new(Mutex::new(Some("Installing…".into())));
        let thread = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            while !token.is_cancelled() {
                std::thread::yield_now();
            }
            // Cancellation after publication must not relabel a committed result.
            tx.send(Finished {
                restore: Vec::new(),
                songs: Some(Vec::new()),
                report: Some(report("1 added".into(), false)),
            })
            .unwrap();
        });
        ready.recv().unwrap();
        let mut imports = Imports {
            job: Some(Job {
                cancel,
                stage,
                done,
                thread,
            }),
            report: None,
            shutting_down: false,
            sent: None,
        };
        assert!(!imports.shutdown());
        assert!(!imports.drained());
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let result = loop {
            if let Some(news) = imports.poll() {
                break news;
            }
            assert!(std::time::Instant::now() < end);
            std::thread::yield_now();
        };
        assert!(imports.drained());
        assert_eq!(result.report.unwrap().summary, "1 added");
        assert!(imports.poll().is_none());
        let mut view = ImportView::default();
        imports.view(&mut view);
        assert!(!view.danoni_cancel);
        assert!(view.running.is_none());
        assert!(!view.report.unwrap().error);
    }

    #[test]
    fn startup_catalog_is_empty_without_a_store_and_needs_no_runtime() {
        let base = std::env::temp_dir().join(format!(
            "ddi-desktop-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let config = HostConfig {
            runtime: Some(base.join("absent-runtime")),
            cache: base.join("cache"),
            staging: base.join("staging"),
            library: base.join("library"),
            offline: true,
            origins: BTreeMap::new(),
        };
        let result = run_config(
            config.clone(),
            None,
            &NetworkCancel::default(),
            &Mutex::default(),
            &|| {},
        )
        .unwrap();
        assert!(result.songs.unwrap().is_empty());
        assert!(!config.library.exists());
        drop(Store::create(&config.library).unwrap());
        let result = run_config(
            config,
            None,
            &NetworkCancel::default(),
            &Mutex::default(),
            &|| {},
        )
        .unwrap();
        assert!(result.songs.unwrap().is_empty());
        assert!(result.report.is_none());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn host_binding_rejects_other_app_and_graph() {
        let mut c = Config {
            host: "host.json".into(),
            application_sha256: sha256(b"app"),
            application_lock_sha256: sha256(b"lock"),
        };
        assert!(check_binding(&c, b"app", b"lock").is_ok());
        assert!(check_binding(&c, b"other", b"lock").is_err());
        c.application_lock_sha256 = sha256(b"other lock");
        assert!(check_binding(&c, b"app", b"lock").is_err());
    }
}
