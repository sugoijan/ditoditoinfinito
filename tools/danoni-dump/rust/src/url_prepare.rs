//! Job-thread URL → source proofs → fresh prepared local songs. Installation is
//! deliberately a separate host transaction; only the outer receipt means Ready.
use crate::{
    Error, Result, assets::*, native, network::NetworkCancel, orchestration, preparation,
    supervisor::WorkerSpec, url_native,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-url-preparation", message)
}
fn wall() -> u64 {
    300000
}
fn bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub source: url_native::UrlRequest,
    pub font: Option<Binding>,
    #[serde(default)]
    pub preparation: preparation::Options,
    #[serde(default = "wall")]
    pub wall_ms: u64,
    #[serde(default = "bytes")]
    pub output_bytes: u64,
}
impl Request {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            source: url_native::UrlRequest::new(url),
            font: None,
            preparation: preparation::Options::default(),
            wall_ms: wall(),
            output_bytes: bytes(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: &'static str,
    pub source: Option<url_native::Progress>,
    pub preparation: Option<preparation::Progress>,
}
pub use url_native::Outcome;
struct Source<'a, S: AssetSource> {
    normalized: native::DirectorySource,
    host: &'a S,
}
impl<S: AssetSource> AssetSource for Source<'_, S> {
    fn read(&self, ns: &str, path: &str, maximum: usize) -> Result<Vec<u8>> {
        if ns == "source" {
            self.normalized.read(ns, path, maximum)
        } else {
            self.host.read(ns, path, maximum)
        }
    }
}
fn check(request: &Request, cancel: &NetworkCancel, started: Instant) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(Error::new("request-cancelled", "URL preparation cancelled"));
    }
    if started.elapsed().as_millis() >= u128::from(request.wall_ms) {
        return Err(Error::new(
            "url-preparation-limit",
            "Whole preparation deadline exceeded",
        ));
    }
    Ok(())
}
fn budget(root: &Path, request: &Request, last: &mut Instant, force: bool) -> Result<()> {
    if force || last.elapsed() >= Duration::from_millis(50) {
        if orchestration::tree_size(root)? > request.output_bytes {
            return Err(Error::new(
                "url-preparation-limit",
                "Whole preparation output limit exceeded",
            ));
        }
        *last = Instant::now();
    }
    Ok(())
}
/// Blocking host API. Host owns cache, trusted worker installation, font bytes,
/// site decisions and cancellation. Output must be a fresh external directory.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    request: &Request,
    cache: &Path,
    worker: &WorkerSpec,
    host: &impl AssetSource,
    output: &Path,
    cancel: &NetworkCancel,
    site: &mut impl FnMut(&str) -> Result<String>,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<Outcome> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(invalid("Call on a blocking job thread"));
    }
    if request.wall_ms == 0
        || request.wall_ms > wall()
        || request.output_bytes == 0
        || request.output_bytes > bytes()
    {
        return Err(invalid("Whole preparation wall/output bounds"));
    }
    if request
        .font
        .as_ref()
        .is_some_and(|b| ["source", "ddi-preparation"].contains(&b.namespace.as_str()))
    {
        return Err(invalid("Reserved font namespace"));
    }
    let started = Instant::now();
    check(request, cancel, started)?;
    let mut owned = orchestration::new_root(output)?;
    let mut last = Instant::now();
    let mut completed = Value::Null;
    let result = (|| {
        let mut emit = |p: Progress| {
            check(request, cancel, started)?;
            budget(&owned.root, request, &mut last, false)?;
            progress(&p)?;
            check(request, cancel, started)
        };
        emit(Progress {
            stage: "source",
            source: None,
            preparation: None,
        })?;
        let source = url_native::execute(
            &request.source,
            cache,
            worker,
            &owned.root.join("source"),
            cancel,
            site,
            &mut |p| {
                emit(Progress {
                    stage: "source",
                    source: Some(p.clone()),
                    preparation: None,
                })
            },
        )?;
        if !source.ready {
            return Err(source
                .failure
                .unwrap_or_else(|| invalid("Source failed without reason")));
        }
        let normalized = &source.manifest["completed"]["normalized"];
        let manifest = Binding {
            namespace: "source".into(),
            path: text(normalized, "manifest")?.into(),
            sha256: text(normalized, "manifestSha256")?.into(),
        };
        relative_path(&manifest.path)?;
        let source = Source {
            normalized: native::DirectorySource::new(BTreeMap::from([(
                "source".into(),
                owned.root.join("source"),
            )]))?,
            host,
        };
        let mut inputs = Inputs::new(&source);
        // The normalization producer emits the scroll review beside its manifest.
        let motion_bytes = inputs.reference(&manifest, "native-scroll-review.json")?;
        let motion = manifest.sibling(&RelativeBinding {
            path: "native-scroll-review.json".into(),
            sha256: sha256(&motion_bytes),
        })?;
        let prepared = preparation::prepare(
            &preparation::Request {
                manifest,
                motion,
                font: request.font.clone(),
                options: request.preparation.clone(),
            },
            &source,
            &mut |p| {
                emit(Progress {
                    stage: "preparation",
                    source: None,
                    preparation: Some(p),
                })
            },
        )?;
        emit(Progress {
            stage: "writing",
            source: None,
            preparation: None,
        })?;
        native::write_prepared(&prepared, &owned.root.join("prepared"), &mut |_| {
            emit(Progress {
                stage: "writing",
                source: None,
                preparation: None,
            })
        })?;
        let source_receipt =
            std::fs::read(owned.root.join("source/manifest.json")).map_err(native::io)?;
        let prepared_receipt =
            std::fs::read(owned.root.join("prepared/manifest.json")).map_err(native::io)?;
        completed = json!({"source":{"path":"source/manifest.json","sha256":sha256(&source_receipt)},
            "prepared":{"path":"prepared/manifest.json","sha256":sha256(&prepared_receipt)},
            "songs":prepared.manifest["songs"].as_array().map_or(0, Vec::len),
            "charts":prepared.manifest["songs"].as_array().into_iter().flatten().map(|s|s["charts"].as_array().map_or(0,Vec::len)).sum::<usize>(),
            "musicalQuality":"provisional","libraryInstalled":false});
        Ok(())
    })();
    let mut failure = result.err();
    if failure.is_some() {
        completed = Value::Null;
    }
    let mut manifest = json!({"schema":"ddi-danoni-url-prepared-run/1","ready":failure.is_none(),
        "request":request,"completed":completed,"failure":failure,
        "scope":"Fresh local candidate songs; no musical approval, index crawl or library installation"});
    if failure.is_none() {
        let body = serde_json::to_vec_pretty(&manifest).map_err(|e| invalid(e.to_string()))?;
        orchestration::write(&owned.root, ".manifest.json.tmp", &body)?;
        failure = budget(&owned.root, request, &mut last, true)
            .and_then(|_| check(request, cancel, started))
            .and_then(|_| {
                progress(&Progress {
                    stage: "ready",
                    source: None,
                    preparation: None,
                })
            })
            .and_then(|_| check(request, cancel, started))
            .and_then(|_| budget(&owned.root, request, &mut last, true))
            .err();
        if failure.is_none() {
            std::fs::rename(
                owned.root.join(".manifest.json.tmp"),
                owned.root.join("manifest.json"),
            )
            .map_err(native::io)?;
        } else {
            std::fs::remove_file(owned.root.join(".manifest.json.tmp")).map_err(native::io)?;
            manifest["ready"] = json!(false);
            manifest["completed"] = Value::Null;
            manifest["failure"] = json!(failure);
        }
    }
    if failure.is_some() {
        orchestration::atomic(&owned.root, "failure.json", &manifest)?;
    }
    owned.retain = true;
    Ok(Outcome {
        ready: failure.is_none(),
        output: owned.root.clone(),
        manifest,
        failure,
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema: String,
    #[serde(default)]
    roots: BTreeMap<String, PathBuf>,
    cache: PathBuf,
    request: Request,
    origins: BTreeMap<String, String>,
}
pub fn execute_job(bundle: &Path, job: &Path, output: &Path) -> Result<Outcome> {
    let path = job.canonicalize().map_err(native::io)?;
    native::external(&path)?;
    let dir = path.parent().unwrap();
    let file = native::DirectorySource::new(BTreeMap::from([("job".into(), dir.to_owned())]))?;
    let job: Job = json(
        &file.read(
            "job",
            path.file_name()
                .unwrap()
                .to_str()
                .ok_or_else(|| invalid("Job name"))?,
            JSON_LIMIT,
        )?,
    )?;
    if job.schema != "ddi-danoni-url-prepared-job/1" {
        return Err(invalid("Unsupported job schema"));
    }
    let source = native::DirectorySource::new(
        job.roots
            .into_iter()
            .map(|(k, p)| (k, if p.is_absolute() { p } else { dir.join(p) }))
            .collect(),
    )?;
    let worker = crate::worker_build::load_bundle(bundle, &mut || Ok(()))?;
    execute(
        &job.request,
        &if job.cache.is_absolute() {
            job.cache
        } else {
            dir.join(job.cache)
        },
        &worker,
        &source,
        output,
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
