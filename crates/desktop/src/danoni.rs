//! The packaged importer runs in an owned child process. Only framed messages
//! cross its anonymous pipes; the SDK and its worker are not linked into the app.
mod bundle;
mod client;

use ddi_importer_protocol::{Paths, Request};
use ddi_ui::screens::data::{DanoniImportView, DanoniOriginRequest, ImportReportView, ImportView};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

const ORIGINS: &str = "ddi.danoni.origins.v1";

pub fn bundle_info() -> serde_json::Value {
    if bundle::ID.is_empty() || bundle::MANIFEST_SHA256.is_empty() {
        return serde_json::json!({"enabled": false});
    }
    serde_json::json!({"enabled": true, "id": bundle::ID,
        "manifestSha256": bundle::MANIFEST_SHA256,
        "protocolVersion": ddi_importer_protocol::VERSION, "video": cfg!(feature = "video")})
}

/// Headless package smoke check through the application's real verified pipe
/// client. No network, window, sound device, or origin decisions are involved.
pub fn check() -> Result<usize, String> {
    let (_controls, requests) = mpsc::channel();
    let finished = client::run(
        &bundle::locate()?,
        paths()?,
        None,
        false,
        BTreeMap::new(),
        &AtomicBool::new(false),
        &Mutex::default(),
        requests,
        &|| {},
    )?;
    if let Some(report) = finished.report.filter(|r| r.error) {
        return Err(report
            .sections
            .into_iter()
            .flat_map(|(_, lines)| lines)
            .collect::<Vec<_>>()
            .join("\n"));
    }
    finished
        .songs
        .map(|songs| songs.len())
        .ok_or_else(|| "Importer did not publish a complete catalog".into())
}

pub struct News {
    pub restore: Vec<String>,
    pub songs: Option<Vec<crate::library::LibSong>>,
    pub report: Option<ImportReportView>,
}
#[derive(Default)]
struct Finished {
    restore: Vec<String>,
    songs: Option<Vec<crate::library::LibSong>>,
    report: Option<ImportReportView>,
}
#[derive(Clone)]
struct Prompt {
    view: DanoniOriginRequest,
    job: u64,
}
#[derive(Default)]
struct Progress {
    work: Option<(u64, u64)>,
    stage: Option<String>,
    origin: Option<Prompt>,
}
struct Job {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    controls: mpsc::SyncSender<Request>,
    done: mpsc::Receiver<Finished>,
    thread: JoinHandle<()>,
}
#[derive(Default)]
pub struct Imports {
    job: Option<Job>,
    report: Option<ImportReportView>,
    url: String,
    offline: bool,
    origins: BTreeMap<String, String>,
    shutting_down: bool,
    sent: Option<ImportView>,
}
impl Imports {
    pub fn view(&self, view: &mut ImportView) {
        view.danoni_urls = !bundle::ID.is_empty() && !bundle::MANIFEST_SHA256.is_empty();
        view.danoni_cancel = self.job.is_some() && !self.shutting_down;
        let progress = self.job.as_ref().and_then(|j| j.progress.lock().ok());
        let stage = progress.as_ref().and_then(|p| p.stage.clone());
        view.running = self.job.as_ref().map(|_| {
            stage
                .clone()
                .unwrap_or_else(|| "Preparing DanOni import…".into())
        });
        view.danoni = DanoniImportView {
            url: self.url.clone(),
            offline: self.offline,
            stage,
            progress: progress.as_ref().and_then(|p| p.work),
            origin: progress
                .as_ref()
                .and_then(|p| p.origin.as_ref().map(|o| o.view.clone())),
        };
        view.report = self.report.clone();
    }
    /// Song select's import view now (as `Songs::data` builds it).
    pub fn current_view(&self) -> ImportView {
        let mut view = ImportView {
            platform: ddi_ui::screens::data::Platform::Desktop,
            ..Default::default()
        };
        self.view(&mut view);
        view
    }
    pub fn changed_view(&mut self) -> Option<ImportView> {
        let view = self.current_view();
        if self.sent.as_ref() == Some(&view) {
            return None;
        }
        self.sent = Some(view.clone());
        Some(view)
    }
    pub fn start(&mut self, url: String, offline: bool, wake: Arc<dyn Fn() + Send + Sync>) {
        if self.job.is_some() || self.shutting_down {
            return;
        }
        self.url = url.trim().to_owned();
        self.offline = offline;
        let parsed = url::Url::parse(&self.url);
        if self.url.len() > 8192
            || !parsed.is_ok_and(|p| {
                matches!(p.scheme(), "http" | "https")
                    && p.host_str().is_some()
                    && p.username().is_empty()
                    && p.password().is_none()
            })
        {
            self.report = Some(report("Enter a complete http:// or https:// DanOni work URL without embedded credentials.".into(), true));
            return;
        }
        self.spawn(Some(self.url.clone()), wake);
    }
    pub fn load(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.origins = ddi_platform::storage::load(ddi_platform::storage::Area::Local, ORIGINS)
            .unwrap_or_default();
        if !bundle::ID.is_empty() && self.job.is_none() && !self.shutting_down {
            self.spawn(None, wake);
        }
    }
    pub fn origin_decision(&mut self, request_id: u64, allow: bool, remember: bool) {
        let Some(job) = &self.job else {
            return;
        };
        let Ok(mut progress) = job.progress.lock() else {
            return;
        };
        let Some(prompt) = progress
            .origin
            .as_ref()
            .filter(|p| p.view.request_id == request_id)
        else {
            return;
        };
        let origin = prompt.view.origin.clone();
        let decision = allow
            .then(|| "User allowed public song files from this origin in Add songs.".to_string());
        if job
            .controls
            .try_send(Request::OriginDecision {
                id: prompt.job,
                origin: origin.clone(),
                decision: decision.clone(),
            })
            .is_err()
        {
            return;
        }
        progress.origin = None;
        drop(progress);
        if allow && remember {
            self.origins.insert(origin, decision.unwrap());
            if let Err(e) = ddi_platform::storage::save(
                ddi_platform::storage::Area::Local,
                ORIGINS,
                &self.origins,
            ) {
                self.report = Some(report(
                    format!("Access allowed for this import; could not remember the decision: {e}"),
                    false,
                ));
            }
        }
    }
    fn spawn(&mut self, url: Option<String>, wake: Arc<dyn Fn() + Send + Sync>) {
        self.report = None;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(Progress {
            work: None,
            stage: Some("Opening DanOni catalog…".into()),
            origin: None,
        }));
        let (controls, requests) = mpsc::sync_channel(4);
        let (tx, done) = mpsc::sync_channel(1);
        let token = cancel.clone();
        let state = progress.clone();
        let origins = self.origins.clone();
        let offline = self.offline;
        match std::thread::Builder::new().name("ddi-danoni-client".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let executable = bundle::locate()?;
                client::run(&executable, paths()?, url, offline, origins, &token, &state, requests, &*wake)
            }));
            let finished = match result {
                Ok(Ok(f)) => f,
                Ok(Err(e)) => Finished { report: Some(report(e, true)), ..Default::default() },
                Err(_) => Finished { report: Some(report("The importer client stopped unexpectedly. Reopen the app to verify its catalog.".into(), true)), ..Default::default() },
            };
            let _ = tx.send(finished); wake();
        }) {
            Ok(thread) => self.job = Some(Job { cancel, progress, controls, done, thread }),
            Err(e) => self.report = Some(report(format!("Cannot start the DanOni importer: {e}"), true)),
        }
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn cancel(&self) {
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Release);
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
            report: Some(report(
                "The importer lost its completion result.".into(),
                true,
            )),
            ..Default::default()
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
        if let Some(job) = self.job.take() {
            // Normal shutdown drains through the event loop. Abnormal teardown
            // transfers ownership, without blocking a UI destructor.
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
fn paths() -> Result<Paths, String> {
    use std::os::unix::fs::DirBuilderExt;
    fn private(path: std::path::PathBuf) -> Result<std::path::PathBuf, String> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)
            .map_err(|e| format!("Cannot create importer storage: {e}"))?;
        if !std::fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            return Err("Importer storage must be a directory without symlinks".into());
        }
        path.canonicalize().map_err(|e| e.to_string())
    }
    let data = crate::paths::data_dir()
        .ok_or("Cannot locate application data directory")?
        .join("danoni");
    let cache = crate::paths::cache_dir()
        .ok_or("Cannot locate application cache directory")?
        .join("danoni");
    let library = private(data)?.join("library");
    Ok(Paths {
        cache: private(cache.join("network"))?,
        staging: private(cache.join("staging"))?,
        library,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origin_answer_is_bound_to_current_prompt_and_enqueued_once() {
        let (controls, received) = mpsc::sync_channel(4);
        let (done_tx, done) = mpsc::sync_channel(1);
        let progress = Arc::new(Mutex::new(Progress {
            work: None,
            stage: None,
            origin: Some(Prompt {
                job: 1,
                view: DanoniOriginRequest {
                    request_id: 42,
                    origin: "https://example.invalid".into(),
                    purpose: "test".into(),
                },
            }),
        }));
        let mut imports = Imports::default();
        imports.job = Some(Job {
            cancel: Arc::new(AtomicBool::new(false)),
            progress: progress.clone(),
            controls,
            done,
            thread: std::thread::spawn(move || {
                let _ = done_tx.send(Finished::default());
            }),
        });
        imports.origin_decision(41, true, false);
        assert!(received.try_recv().is_err());
        assert!(progress.lock().unwrap().origin.is_some());
        imports.origin_decision(42, true, false);
        assert!(
            matches!(received.try_recv().unwrap(), Request::OriginDecision { id: 1, origin, decision: Some(_) } if origin == "https://example.invalid")
        );
        assert!(progress.lock().unwrap().origin.is_none());
        imports.origin_decision(42, true, false);
        assert!(received.try_recv().is_err());
        imports.shutdown();
        let job = imports.job.take().unwrap();
        job.thread.join().unwrap();
    }
    #[test]
    fn malformed_url_does_not_launch_a_helper() {
        let mut imports = Imports::default();
        for url in [
            "file:///secret",
            "https://name:password@example.invalid",
            "not a URL",
        ] {
            imports.start(url.into(), false, Arc::new(|| {}));
            assert!(!imports.busy());
            assert!(imports.report.as_ref().unwrap().error);
        }
    }
}
