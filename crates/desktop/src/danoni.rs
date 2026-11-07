//! The packaged importer runs in an owned child process. Only framed messages
//! cross its anonymous pipes; the SDK and its worker are not linked into the app.
//! What it reports reaches the screens as values (`report`), said in the
//! player's language where the import view is made.
mod bundle;
mod client;
mod report;

use ddi_importer_protocol::{Paths, Request};
use ddi_ui::screens::data::{DanoniImportView, DanoniOriginRequest, ImportView};
use report::{Problem, Report, Step, Summary, SystemError};
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
/// What went wrong is written for the terminal: in English.
pub fn check() -> Result<usize, String> {
    ddi_l10n::set_language("en");
    let english = |problem: Problem| problem.text().to_string();
    let (_controls, requests) = mpsc::channel();
    let finished = client::run(
        &bundle::locate().map_err(english)?,
        paths().map_err(english)?,
        None,
        false,
        BTreeMap::new(),
        &AtomicBool::new(false),
        &Mutex::default(),
        requests,
        &|| {},
    )
    .map_err(english)?;
    if let Some(report) = finished.report.filter(Report::error) {
        return Err(report
            .lines()
            .map(String::from)
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
}
#[derive(Default)]
struct Finished {
    restore: Vec<String>,
    songs: Option<Vec<crate::library::LibSong>>,
    report: Option<Report>,
}
#[derive(Clone)]
struct Prompt {
    view: DanoniOriginRequest,
    job: u64,
}
#[derive(Default)]
struct Progress {
    work: Option<(u64, u64)>,
    stage: Option<Step>,
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
    report: Option<Report>,
    url: String,
    offline: bool,
    origins: BTreeMap<String, String>,
    shutting_down: bool,
    sent: Option<ImportView>,
}
impl Imports {
    /// The import's part of song select's import view, said in the
    /// language now (made again each time: a change of language is sent
    /// as a change of the view).
    pub fn view(&self, view: &mut ImportView) {
        view.danoni_urls = !bundle::ID.is_empty() && !bundle::MANIFEST_SHA256.is_empty();
        view.danoni_cancel = self.job.is_some() && !self.shutting_down;
        let progress = self.job.as_ref().and_then(|j| j.progress.lock().ok());
        let step = progress.as_ref().and_then(|p| p.stage);
        let stage = step.map(|s| String::from(s.text()));
        view.running = self.job.as_ref().map(|_| {
            ddi_ui::screens::data::ImportRunning::Worded(
                stage
                    .clone()
                    .unwrap_or_else(|| Step::Preparing.text().into()),
            )
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
        view.report = self.report.as_ref().map(Report::view);
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
            self.report = Some(Report::failed(Problem::InvalidUrl));
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
                self.report = Some(Report::new(Summary::NotRemembered(e)));
            }
        }
    }
    fn spawn(&mut self, url: Option<String>, wake: Arc<dyn Fn() + Send + Sync>) {
        self.report = None;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(Progress {
            work: None,
            stage: Some(Step::OpeningCatalog),
            origin: None,
        }));
        let (controls, requests) = mpsc::sync_channel(4);
        let (tx, done) = mpsc::sync_channel(1);
        let token = cancel.clone();
        let state = progress.clone();
        let origins = self.origins.clone();
        let offline = self.offline;
        match std::thread::Builder::new()
            .name("ddi-danoni-client".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let executable = bundle::locate()?;
                    client::run(
                        &executable,
                        paths()?,
                        url,
                        offline,
                        origins,
                        &token,
                        &state,
                        requests,
                        &*wake,
                    )
                }));
                let failed = |problem| Finished {
                    report: Some(Report::failed(problem)),
                    ..Default::default()
                };
                let finished = match result {
                    Ok(Ok(f)) => f,
                    Ok(Err(problem)) => failed(problem),
                    Err(_) => failed(Problem::ClientStopped),
                };
                let _ = tx.send(finished);
                wake();
            }) {
            Ok(thread) => {
                self.job = Some(Job {
                    cancel,
                    progress,
                    controls,
                    done,
                    thread,
                })
            }
            Err(e) => self.report = Some(Report::failed(Problem::Start(SystemError::from(e)))),
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
            report: Some(Report::failed(Problem::LostResult)),
            ..Default::default()
        });
        let _ = job.thread.join();
        self.report = result.report;
        Some(News {
            restore: result.restore,
            songs: result.songs,
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
fn paths() -> Result<Paths, Problem> {
    use std::os::unix::fs::DirBuilderExt;
    fn private(path: std::path::PathBuf) -> Result<std::path::PathBuf, Problem> {
        let storage = |e| Problem::Storage(SystemError::from(e));
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)
            .map_err(storage)?;
        if !std::fs::symlink_metadata(&path).map_err(storage)?.is_dir() {
            return Err(Problem::StorageNotFolder);
        }
        path.canonicalize().map_err(storage)
    }
    let data = crate::paths::data_dir()
        .ok_or(Problem::NoDataFolder)?
        .join("danoni");
    let cache = crate::paths::cache_dir()
        .ok_or(Problem::NoCacheFolder)?
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
            assert_eq!(
                imports.report.as_ref(),
                Some(&Report::failed(Problem::InvalidUrl))
            );
        }
    }
    #[test]
    fn the_view_says_the_import_in_the_language_now() {
        let (controls, _requests) = mpsc::sync_channel(4);
        let (done_tx, done) = mpsc::sync_channel(1);
        let progress = Arc::new(Mutex::new(Progress {
            work: Some((2, 5)),
            stage: Some(Step::Importer(
                ddi_importer_protocol::Stage::ConvertingMusic,
            )),
            origin: None,
        }));
        let mut imports = Imports::default();
        imports.report = Some(Report::failed(Problem::TimedOut));
        imports.job = Some(Job {
            cancel: Arc::new(AtomicBool::new(false)),
            progress,
            controls,
            done,
            thread: std::thread::spawn(move || {
                let _ = done_tx.send(Finished::default());
            }),
        });
        ddi_l10n::set_language("en");
        let view = imports.changed_view().unwrap();
        assert_eq!(view.danoni.stage.as_deref(), Some("Converting music…"));
        assert_eq!(
            view.running,
            view.danoni
                .stage
                .clone()
                .map(ddi_ui::screens::data::ImportRunning::Worded)
        );
        assert_eq!(view.danoni.progress, Some((2, 5)));
        let report = view.report.unwrap();
        assert_eq!(report.summary(), "DanOni import could not finish.");
        assert!(report.error());
        assert!(imports.changed_view().is_none(), "nothing changed");
        ddi_l10n::set_language("ja");
        let view = imports.changed_view().expect("a change of language");
        assert_eq!(view.danoni.stage.as_deref(), Some("音楽を変換中…"));
        ddi_l10n::set_language("en");
        imports.shutdown();
        imports.job.take().unwrap().thread.join().unwrap();
    }
}
