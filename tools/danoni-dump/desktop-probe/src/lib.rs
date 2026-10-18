//! Experimental SDK → desktop handoff, without adding the SDK to the game.
//! Uses a verified committed catalog, never a recursive scan of transactions.
use ddi_danoni_export::{
    Error, Result, import_job, managed_store::Store, native::DirectorySource,
    network::NetworkCancel, runtime,
};
use ddi_desktop::{
    audio::Audio,
    library::LibSong,
    play::{Ended, Feed, Play, Request, load},
};
use ddi_platform::{HostTime, history::MemoryPlayStore};
use ddi_ui::state::{history::Plays, settings::Settings};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
};

fn error(message: impl ToString) -> Error {
    Error {
        code: "desktop-probe",
        message: message.to_string(),
    }
}

/// Test host: bounded/coalesced progress, separate non-lossy completion, and an
/// explicit cancellation handle. Dropping the handle requests cancellation;
/// an event-loop host must poll completion instead of joining on its UI thread.
pub struct Job {
    pub cancel: NetworkCancel,
    progress: Arc<Mutex<Option<Value>>>,
    thread: Option<JoinHandle<Result<import_job::Outcome>>>,
}
impl Job {
    pub fn spawn(
        run: impl FnOnce(
            &NetworkCancel,
            &mut dyn FnMut(&import_job::Progress) -> Result<()>,
        ) -> Result<import_job::Outcome>
        + Send
        + 'static,
    ) -> Result<Self> {
        let cancel = NetworkCancel::default();
        let control = cancel.clone();
        let progress = Arc::new(Mutex::new(None));
        let latest = progress.clone();
        let thread = thread::Builder::new()
            .name("danoni-import-probe".into())
            .spawn(move || {
                run(&control, &mut |p| {
                    // Do not hold full per-song receipts in a progress queue.
                    let stage = match p {
                        import_job::Progress::Preparing { .. } => "preparing",
                        import_job::Progress::Validating { .. } => "validating",
                        import_job::Progress::Planned { .. } => "planned",
                        import_job::Progress::Installing { .. } => "installing",
                    };
                    *latest.lock().map_err(error)? = Some(json!({"stage":stage}));
                    Ok(())
                })
            })
            .map_err(error)?;
        Ok(Self {
            cancel,
            progress,
            thread: Some(thread),
        })
    }
    pub fn take_progress(&self) -> Option<Value> {
        self.progress.lock().ok()?.take()
    }
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
    /// Blocking only for the test/CLI. Desktop should join after is_finished().
    pub fn finish(mut self) -> Result<import_job::Outcome> {
        self.thread
            .take()
            .ok_or_else(|| error("Job already consumed"))?
            .join()
            .map_err(|_| error("Import thread panicked"))?
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub fn resume_job(candidate: import_job::Candidate, library: PathBuf) -> Result<Job> {
    Job::spawn(move |cancel, progress| {
        let mut store = Store::open(&library)?;
        import_job::resume(
            &import_job::Resume::new(candidate),
            &mut store,
            cancel,
            &mut |p| progress(p),
        )
    })
}

/// Host paths/site decisions remain trusted configuration; only `url` comes
/// from the user. Mirrors the installed runtime defaults, with host-owned
/// scheduling/progress/cancellation instead of the CLI convenience wrapper.
pub fn url_job(config: runtime::HostConfig, url: String, output: PathBuf) -> Result<Job> {
    Job::spawn(move |cancel, progress| {
        let installation = runtime::Installation::locate(config.runtime.as_deref())?;
        let assets = DirectorySource::new(BTreeMap::new())?;
        let mut store = Store::open(&config.library)?;
        import_job::execute(
            &installation.request(url, config.offline),
            &import_job::Services {
                cache: &config.cache,
                worker: &installation.worker,
                assets: &assets,
            },
            &output,
            &mut store,
            cancel,
            &mut |u| {
                let origin = url::Url::parse(u)
                    .map_err(error)?
                    .origin()
                    .ascii_serialization();
                config
                    .origins
                    .get(&origin)
                    .filter(|s| !s.trim().is_empty())
                    .cloned()
                    .ok_or_else(|| Error {
                        code: "site-terms-denied",
                        message: format!("No host decision for {origin}"),
                    })
            },
            &mut |p| progress(p),
        )
    })
}

/// The store must be idle and valid before exposing any row. This prototype
/// knows the store v1 path layout. A production adapter should get these paths
/// through a store API and merge rows with desktop's folder registry.
/// The store is private/cooperatively written; it is not a hostile-file sandbox.
pub fn snapshot(root: &Path) -> Result<(String, Vec<LibSong>)> {
    let root = root.canonicalize().map_err(error)?;
    let store = Store::open(&root)?;
    let catalog = store.verify()?;
    let songs = catalog
        .songs
        .into_iter()
        .map(|s| LibSong {
            root: root
                .join("transactions")
                .join(&s.transaction)
                .join("songs")
                .join(&s.installed.id),
            zip: false,
            dir: Default::default(),
            pack: "DanOni imports".into(),
            entry: s.entry,
            warnings: vec![],
            notes: vec![],
            work: None, // Prepared SSC, not a raw HTML5 work.
        })
        .collect();
    Ok((catalog.revision, songs))
}

/// Real desktop loading/decoding and silent fake-clock play. No window, GPU,
/// output device or persisted user history. Test registry replaces all rows.
pub fn audit(songs: &[LibSong]) -> Result<Value> {
    ddi_desktop::library::register_for_tests(songs);
    let mut reports = Vec::new();
    for s in songs {
        if s.zip || s.work.is_some() {
            return Err(error("Probe expects prepared SSC folders"));
        }
        let loaded = Rc::new(load::load(&s.entry.id, 48_000).map_err(error)?);
        if loaded.entry != s.entry
            || loaded.song.charts.len() != s.entry.charts.len()
            || loaded.music.frames() == 0
            || loaded.music.data.iter().any(|v| !v.is_finite())
        {
            return Err(error("Desktop load differs from the admitted catalog"));
        }
        let dir = s
            .dir
            .to_path(&s.root)
            .ok_or_else(|| error("Invalid song directory"))?;
        let preview = ddi_desktop::preview::PreviewSong::from_dir(&s.entry.id, &dir)
            .ok_or_else(|| error("Preview lookup failed"))?;
        if (preview.from - s.entry.preview_start.max(0.0)).abs() > 1e-6 || !preview.music.is_file()
        {
            return Err(error("Preview metadata/path differs"));
        }
        let banner = s
            .entry
            .banner
            .as_ref()
            .map(|name| {
                ddi_desktop::images::load(&s.source(name), 1024)
                    .map(|(size, _)| size)
                    .ok_or_else(|| error("Banner decode failed"))
            })
            .transpose()?;
        let mut charts = Vec::new();
        for chart in &s.entry.charts {
            let parsed = &loaded.song.charts[chart.index];
            if parsed.judged_note_count() != chart.notes || parsed.notes.is_empty() {
                return Err(error("Parsed notes differ from catalog or chart is empty"));
            }
            let layout = loaded
                .song
                .layout_of(parsed)
                .ok_or_else(|| error("Unsupported desktop layout"))?;
            let mut runs = Vec::new();
            for fps in [60u64, 144] {
                let clock = Arc::new(AtomicU64::new(1_000_000_000));
                let c = clock.clone();
                let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
                let history = Rc::new(MemoryPlayStore::default());
                let plays = Plays::new(history.clone());
                let mut p = Play::start(
                    Request::song(&s.entry.id, chart.index, true),
                    loaded.clone(),
                    &Settings::default(),
                    audio,
                    Rc::new(RefCell::new(Feed::default())),
                    None,
                    fps as f64,
                )
                .map_err(error)?;
                let limit =
                    (loaded.music.duration().max(s.entry.length) + 120.0).ceil() as u64 * fps;
                let mut result = None;
                for _ in 0..limit {
                    let now = clock.fetch_add(1_000_000_000 / fps, Ordering::SeqCst)
                        + 1_000_000_000 / fps;
                    match p.tick(HostTime(now as f64 / 1e9), Some(&plays)) {
                        Some(Ended::Finished { results, .. }) => {
                            result = Some(results);
                            break;
                        }
                        Some(_) => return Err(error("Autoplay quit before results")),
                        None => (),
                    }
                }
                let result = result.ok_or_else(|| error("Autoplay did not finish within bound"))?;
                if result.tally.taps[0] == 0
                    || result.failed
                    || result.tally.taps[1..].iter().sum::<u32>() != 0
                    || result.tally.let_go != 0
                    || history.len() != 1
                {
                    return Err(error(format!(
                        "{} chart {} at {fps} Hz: failed autoplay/history",
                        s.entry.title, chart.index
                    )));
                }
                runs.push(
                    json!({"fps":fps,"topTier":result.tally.taps[0],"held":result.tally.held,
                    "letGo":result.tally.let_go,"historyRecords":history.len()}),
                );
            }
            charts.push(json!({"index":chart.index,"layout":layout.id,"runs":runs}));
        }
        reports.push(
            json!({"id":s.entry.id,"title":s.entry.title,"bpm":s.entry.bpm,
            "previewStart":preview.from,"previewLength":preview.length,"bannerSize":banner,
            "audioFrames":loaded.music.frames(),"audioRate":loaded.music.rate,
            "audioChannels":loaded.music.channels,"charts":charts}),
        );
    }
    Ok(
        json!({"songs":reports,"scope":"Real desktop loader, decoder and headless play; no GPU, audible output or shell UI"}),
    )
}
