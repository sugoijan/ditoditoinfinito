//! Offline selector/capture orchestration. One bound control config per chart
//! drives every role; observations can vary, script inputs cannot. No discovery
//! or fetch occurs here, and the installed worker is always supplied by the host.
use crate::{
    Error, Result,
    assets::*,
    extraction::*,
    model::Metadata,
    native::{self, DirectorySource},
    supervisor::{self, CaptureRequest, Limits, WorkerSpec},
    swf,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};
const NAMESPACE: &str = "plan";
const CONFIG_LIMIT: usize = 65536;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub capture_frames: Vec<u64>,
    #[serde(default)]
    pub render: bool,
    #[serde(default)]
    pub mix_audio: bool,
    pub avm_execution_ms: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartPlan {
    pub id: String,
    pub label: Option<String>,
    pub options: ProjectionOptions,
    pub config: Binding,
    pub projection: Observation,
    pub motion: Observation,
    pub motion_selection: MotionSelection,
    pub audio: Observation,
    pub stream_start_index: usize,
    pub optional_color: Option<OptionalColorEvidence>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativePlan {
    pub source_manifest: Binding,
    pub movie: Binding,
    #[serde(default)]
    pub metadata: Metadata,
    pub charts: Vec<ChartPlan>,
    #[serde(default)]
    pub limits: Limits,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub plan: NativePlan,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedCapture {
    pub id: String,
    pub request: CaptureRequest,
}
struct PlannedChart {
    plan: ChartPlan,
    roles: [String; 3],
    optional: Option<OptionalColorEvidence>,
}
/// Immutable prepared inputs plus a reviewable capture inventory. Execute takes
/// the original request and replans, never trusts deserialized prepared state.
pub struct PreparedPlan {
    pub manifest: Value,
    pub files: Vec<Artifact>,
    pub captures: Vec<PlannedCapture>,
    charts: Vec<PlannedChart>,
    movie: Binding,
    provenance: Binding,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-native-plan", message)
}
fn bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|e| invalid(e.to_string()))
}
fn bind(path: &str, b: &[u8]) -> Binding {
    Binding {
        namespace: NAMESPACE.into(),
        path: path.into(),
        sha256: sha256(b),
    }
}
fn add(
    files: &mut BTreeMap<String, Arc<Vec<u8>>>,
    path: String,
    b: Arc<Vec<u8>>,
) -> Result<Binding> {
    relative_path(&path)?;
    let binding = bind(&path, &b);
    if let Some(old) = files.get(&path) {
        if **old != *b {
            return Err(invalid("Conflicting planned path"));
        }
    } else {
        if files.len() >= 4096
            || files
                .values()
                .map(|b| b.len())
                .sum::<usize>()
                .checked_add(b.len())
                .is_none_or(|v| v > 128 * 1024 * 1024)
        {
            return Err(invalid("Plan input inventory exceeds 4096 files / 128 MiB"));
        }
        files.insert(path, b);
    }
    Ok(binding)
}
fn blob(files: &mut BTreeMap<String, Arc<Vec<u8>>>, b: Arc<Vec<u8>>) -> Result<Binding> {
    add(files, format!("inputs/blobs/{}", sha256(&b)), b)
}
fn json_file(
    files: &mut BTreeMap<String, Arc<Vec<u8>>>,
    path: String,
    v: &impl Serialize,
) -> Result<Binding> {
    let b = bytes(v)?;
    if b.len() > JSON_LIMIT {
        return Err(invalid("Plan JSON limit"));
    }
    add(files, path, Arc::new(b))
}
pub(crate) fn optional(
    e: &OptionalColorEvidence,
    id: &str,
    inputs: &mut Inputs<'_, impl AssetSource>,
    files: &mut BTreeMap<String, Arc<Vec<u8>>>,
) -> Result<OptionalColorEvidence> {
    let raw = inputs.checked(&e.review, JSON_LIMIT)?;
    add(
        files,
        format!("inputs/original/{id}-color.json"),
        raw.clone(),
    )?;
    let mut review: Value = json(&raw)?;
    let rows = review["missingResources"]
        .as_array_mut()
        .ok_or_else(|| invalid("Missing color response inventory"))?;
    if rows.len() != e.missing_responses.len() || rows.len() > 64 {
        return Err(invalid("Color response coverage differs"));
    }
    let mut responses = vec![];
    for row in rows {
        let matching = e
            .missing_responses
            .iter()
            .filter(|b| row["path"] == b.path && row["sha256"] == b.sha256)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(invalid("Unbound color response"));
        }
        let b = inputs.checked(matching[0], EVIDENCE_LIMIT)?;
        let dest = blob(files, b)?;
        row["path"] = json!(dest.path);
        responses.push(dest);
    }
    Ok(OptionalColorEvidence {
        review: json_file(files, format!("inputs/{id}-color.json"), &review)?,
        missing_responses: responses,
    })
}
/// Validate and snapshot every immutable source input before any worker launch.
/// Deduplicate only byte-identical generated configurations. Selectors and
/// FlashVars remain exactly those of the preserved per-chart configuration.
pub fn prepare_plan(
    plan: &NativePlan,
    source: &impl AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<PreparedPlan> {
    checkpoint()?;
    plan.limits.validate()?;
    if plan.charts.is_empty() || plan.charts.len() > 16 {
        return Err(invalid("Need 1..16 explicitly selected charts"));
    }
    let mut ids = BTreeSet::new();
    for row in &plan.charts {
        if row.id.is_empty()
            || row.id.len() > 128
            || !row
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !ids.insert(row.id.to_ascii_lowercase())
            || row.label.as_ref().is_some_and(|s| s.len() > 4096)
        {
            return Err(invalid("Invalid or duplicate chart identity"));
        }
        if !row.audio.mix_audio
            || row.motion_selection.iterations.is_empty()
            || !row
                .motion_selection
                .iterations
                .iter()
                .all(|f| row.motion.capture_frames.contains(f))
        {
            return Err(invalid(
                "Audio must mix PCM; selected motion frames must be observed",
            ));
        }
    }
    let mut inputs = Inputs::new(source);
    let mut files = BTreeMap::new();
    let provenance_bytes = inputs.checked(&plan.source_manifest, JSON_LIMIT)?;
    let provenance_value: Value = json(&provenance_bytes)?;
    if provenance_value
        .get("sourceSwfSha256")
        .unwrap_or(&provenance_value["swf"]["sha256"])
        .as_str()
        != Some(plan.movie.sha256.as_str())
    {
        return Err(invalid("Source provenance belongs to another movie"));
    }
    let provenance = add(
        &mut files,
        "inputs/source-manifest.json".into(),
        provenance_bytes,
    )?;
    let original = inputs.checked(&plan.movie, swf::MOVIE_LIMIT)?;
    if swf::Movie::read(&original, checkpoint)?.header.fps != 60. {
        return Err(invalid("Normalized native plans require 60 Hz"));
    }
    let movie = blob(&mut files, original)?;
    let mut captures = BTreeMap::new();
    let mut charts = vec![];
    for row in &plan.charts {
        checkpoint()?;
        let raw = inputs.checked(&row.config, CONFIG_LIMIT)?;
        supervisor::validate_config(&raw)?;
        add(
            &mut files,
            format!("inputs/original/{}.json", row.id),
            raw.clone(),
        )?;
        let mut config: Value = json(&raw)?;
        let name = config["movie"]
            .as_str()
            .ok_or_else(|| invalid("Configured movie name"))?;
        let configured = row.config.sibling(&RelativeBinding {
            path: name.into(),
            sha256: plan.movie.sha256.clone(),
        })?;
        inputs.checked(&configured, swf::MOVIE_LIMIT)?;
        // A different contained path can name the same immutable movie. Both
        // original bindings are recorded; the staged role configs use one blob.
        config["movie"] = json!(format!("blobs/{}", movie.sha256));
        if config.get("resources").is_none() {
            config["resources"] = json!([]);
        }
        if let Some(resources) = config["resources"].as_array_mut() {
            let mut total = 0;
            for resource in resources {
                let relative: RelativeBinding = serde_json::from_value(
                    json!({"path":resource["path"],"sha256":resource["sha256"]}),
                )
                .map_err(|e| invalid(e.to_string()))?;
                let b = inputs.checked(&row.config.sibling(&relative)?, 16 * 1024 * 1024)?;
                total += b.len();
                if total > 64 * 1024 * 1024 {
                    return Err(invalid("Chart resource byte limit"));
                }
                let dest = blob(&mut files, b)?;
                resource["path"] = json!(format!("blobs/{}", dest.sha256));
            }
        }
        let opt = row
            .optional_color
            .as_ref()
            .map(|e| optional(e, &row.id, &mut inputs, &mut files))
            .transpose()?;
        let mut roles = vec![];
        for observation in [&row.projection, &row.motion, &row.audio] {
            let mut c = config.clone();
            c["captureFrames"] = json!(observation.capture_frames);
            c["render"] = json!(observation.render);
            c["mixAudio"] = json!(observation.mix_audio);
            if let Some(ms) = observation.avm_execution_ms {
                c["avmExecutionMs"] = json!(ms);
            }
            let b = bytes(&c)?;
            supervisor::validate_config(&b)?;
            let hash = sha256(&b);
            let id = format!("capture-{hash}");
            let binding = add(&mut files, format!("inputs/{id}.json"), Arc::new(b))?;
            captures
                .entry(id.clone())
                .or_insert_with(|| PlannedCapture {
                    id: id.clone(),
                    request: CaptureRequest {
                        config: binding,
                        movie: movie.clone(),
                        provenance: Some(provenance.clone()),
                        limits: plan.limits.clone(),
                    },
                });
            roles.push(id);
        }
        if captures.len() > 32 {
            return Err(invalid("Plan exceeds 32 distinct captures"));
        }
        charts.push(PlannedChart {
            plan: row.clone(),
            roles: roles.try_into().unwrap(),
            optional: opt,
        });
    }
    json_file(&mut files, "plan.json".into(), plan)?;
    let captures = captures.into_values().collect::<Vec<_>>();
    // A successful local dump can be repeated using only its own staged bytes.
    // This changes file bindings, never selector/script inputs or source URLs.
    let mut replay = plan.clone();
    replay.movie = movie.clone();
    replay.source_manifest = provenance.clone();
    for (row, compiled) in replay.charts.iter_mut().zip(&charts) {
        row.config = captures
            .iter()
            .find(|c| c.id == compiled.roles[0])
            .unwrap()
            .request
            .config
            .clone();
        row.optional_color = compiled.optional.clone();
    }
    json_file(
        &mut files,
        "replay-job.json".into(),
        &PlanJob {
            schema: "ddi-danoni-native-plan-job/1".into(),
            roots: BTreeMap::from([(NAMESPACE.into(), PathBuf::from("."))]),
            plan: replay,
        },
    )?;

    let manifest = json!({"schema":"ddi-danoni-prepared-native-plan/1","producer":"native-plan-orchestration/1","request":plan,"sourceInputs":inputs.used,"movie":movie,"provenance":provenance,"captures":captures,"charts":charts.iter().map(|c|json!({"id":c.plan.id,"projection":c.roles[0],"motion":c.roles[1],"audio":c.roles[2],"optionalColor":c.optional})).collect::<Vec<_>>(),"artifacts":files.iter().map(|(path,b)|json!({"path":path,"sha256":sha256(b),"bytes":b.len()})).collect::<Vec<_>>(),"scope":"Explicit offline selector plan; no selector discovery, acquisition or library installation"});
    checkpoint()?;
    Ok(PreparedPlan {
        manifest,
        files: files
            .into_iter()
            .map(|(path, bytes)| Artifact { path, bytes })
            .collect(),
        captures,
        charts,
        movie,
        provenance,
    })
}
pub(crate) struct Owned {
    pub(crate) root: PathBuf,
    pub(crate) retain: bool,
}
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.retain {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
pub(crate) fn new_root(path: &Path) -> Result<Owned> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(native::io)?;
    native::external(&parent)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| invalid("Output name"))?;
    relative_path(name)?;
    let root = parent.join(name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .map_err(native::io)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(&root).map_err(native::io)?;
    Ok(Owned {
        root,
        retain: false,
    })
}
pub(crate) fn write(root: &Path, path: &str, b: &[u8]) -> Result<()> {
    relative_path(path)?;
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).map_err(native::io)?;
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(p)
        .map_err(native::io)?
        .write_all(b)
        .map_err(native::io)
}
pub(crate) fn atomic(root: &Path, name: &str, value: &Value) -> Result<()> {
    let tmp = match name.rsplit_once('/') {
        Some((parent, name)) => format!("{parent}/.{name}.tmp"),
        None => format!(".{name}.tmp"),
    };
    write(root, &tmp, &bytes(value)?)?;
    fs::rename(root.join(tmp), root.join(name)).map_err(native::io)
}
fn bound_file(root: &Path, path: &str) -> Result<Binding> {
    let source = DirectorySource::new(BTreeMap::from([(NAMESPACE.into(), root.to_path_buf())]))?;
    let b = source.read(NAMESPACE, path, 64 * 1024 * 1024)?;
    Ok(bind(path, &b))
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: &'static str,
    pub capture_id: Option<String>,
    pub completed: usize,
    pub total: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOutcome {
    pub ready: bool,
    pub output: PathBuf,
    pub manifest: Value,
    pub failure: Option<Error>,
}
pub(crate) fn tree_size(root: &Path) -> Result<u64> {
    let mut n = 0u64;
    for e in fs::read_dir(root).map_err(native::io)? {
        let p = e.map_err(native::io)?.path();
        let m = fs::symlink_metadata(&p).map_err(native::io)?;
        if m.file_type().is_symlink() {
            return Err(invalid("Unexpected plan output symlink"));
        }
        n = n
            .checked_add(if m.is_dir() {
                tree_size(&p)?
            } else if m.is_file() {
                m.len()
            } else {
                return Err(invalid("Nonregular plan output"));
            })
            .ok_or_else(|| invalid("Output size overflow"))?;
        if n > 2 * 1024 * 1024 * 1024 {
            return Err(invalid("Plan output exceeds 2 GiB"));
        }
    }
    Ok(n)
}
fn reserve(root: &Path, extra: u64) -> Result<()> {
    if tree_size(root)?
        .checked_add(extra)
        .is_none_or(|n| n > 2 * 1024 * 1024 * 1024)
    {
        return Err(invalid("Plan exceeds reserved 2 GiB output budget"));
    }
    Ok(())
}
pub(crate) fn worker_bytes(worker: &WorkerSpec) -> Result<u64> {
    let mut total = 0;
    for (path, max) in [
        (&worker.executable, 256 * 1024 * 1024),
        (&worker.build_identity, 65536),
    ] {
        let m = fs::symlink_metadata(path).map_err(native::io)?;
        if !m.is_file() || m.file_type().is_symlink() || m.len() > max {
            return Err(invalid("Invalid installed worker file"));
        }
        total += m.len();
    }
    Ok(total)
}
/// Capture all selected roles and normalize one work. Input failures occur before
/// output creation; later failures retain diagnostics without a ready marker.
/// Every chart must succeed for this work to be ready; subsets must be explicit.
pub fn execute(
    plan: &NativePlan,
    worker: &WorkerSpec,
    source: &impl AssetSource,
    output: &Path,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<PlanOutcome> {
    let total = plan.charts.len() * 3;
    progress(&Progress {
        stage: "planning",
        capture_id: None,
        completed: 0,
        total,
    })?;
    let prepared = prepare_plan(plan, source, &mut || {
        progress(&Progress {
            stage: "planning",
            capture_id: None,
            completed: 0,
            total,
        })
    })?;
    let mut owned = new_root(output)?;
    for (index, file) in prepared.files.iter().enumerate() {
        progress(&Progress {
            stage: "staging",
            capture_id: None,
            completed: index,
            total: prepared.files.len(),
        })?;
        write(&owned.root, &file.path, &file.bytes)?;
    }
    atomic(&owned.root, "planning.json", &prepared.manifest)?;
    fs::create_dir(owned.root.join("captures")).map_err(native::io)?;
    let source = DirectorySource::new(BTreeMap::from([(NAMESPACE.into(), owned.root.clone())]))?;
    let mut runs = BTreeMap::new();
    let mut stage = "capture";
    let result = (|| -> Result<Value> {
        let runtime_bytes = worker_bytes(worker)?;
        let input_bytes = prepared
            .files
            .iter()
            .map(|f| f.bytes.len() as u64)
            .sum::<u64>();
        for (index, capture) in prepared.captures.iter().enumerate() {
            // Reserve another capture's staged inputs/runtime/output, metadata,
            // and the normalizer's reviewed 512 MiB maximum before launching.
            reserve(
                &owned.root,
                input_bytes
                    + runtime_bytes
                    + capture.request.limits.output_bytes
                    + 2 * 1024 * 1024
                    + 576 * 1024 * 1024,
            )?;
            let event = Progress {
                stage: "capture",
                capture_id: Some(capture.id.clone()),
                completed: index,
                total: prepared.captures.len(),
            };
            let outcome = supervisor::capture(
                &capture.request,
                worker,
                &source,
                &owned.root.join("captures").join(&capture.id),
                &mut || progress(&event),
            )?;
            let capture_manifest = outcome
                .manifest
                .as_ref()
                .map(|_| format!("captures/{}/manifest.json", capture.id));
            let capture_manifest_sha256 = capture_manifest
                .as_ref()
                .map(|path| bound_file(&owned.root, path).map(|b| b.sha256))
                .transpose()?;
            runs.insert(capture.id.clone(),json!({"ready":outcome.ready,"failure":outcome.failure,"manifest":capture_manifest,"manifestSha256":capture_manifest_sha256,"run":outcome.run}));
            if let Some(e) = outcome.failure {
                return Err(e);
            }
            tree_size(&owned.root)?;
        }
        stage = "normalization";
        reserve(&owned.root, 576 * 1024 * 1024)?;
        let capture_binding = |id: &str| -> Result<CaptureBindings> {
            let c = prepared
                .captures
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| invalid("Missing planned capture"))?;
            Ok(CaptureBindings {
                snapshot: bound_file(&owned.root, &format!("captures/{id}/capture/snapshot.json"))?,
                run: bound_file(&owned.root, &format!("captures/{id}/run.json"))?,
                config: c.request.config.clone(),
            })
        };
        let mut charts = vec![];
        for c in &prepared.charts {
            charts.push(NativeChartRequest {
                id: c.plan.id.clone(),
                label: c.plan.label.clone(),
                options: c.plan.options.clone(),
                projection: capture_binding(&c.roles[0])?,
                motion: capture_binding(&c.roles[1])?,
                audio: capture_binding(&c.roles[2])?,
                motion_selection: c.plan.motion_selection.clone(),
                native_pcm: bound_file(
                    &owned.root,
                    &format!("captures/{}/capture/audio.pcm", c.roles[2]),
                )?,
                stream_start_index: c.plan.stream_start_index,
                optional_color: c.optional.clone(),
            });
        }
        let request = NativeWorkRequest {
            source_manifest: prepared.provenance.clone(),
            movie: prepared.movie.clone(),
            metadata: plan.metadata.clone(),
            charts,
        };
        write(&owned.root, "normalization-request.json", &bytes(&request)?)?;
        let event = Progress {
            stage: "normalization",
            capture_id: None,
            completed: prepared.captures.len(),
            total: prepared.captures.len(),
        };
        let work = prepare_native_work(&request, &source, &mut || progress(&event))?;
        native::write_normalized(&work, &owned.root.join("normalized"), &mut || {
            progress(&event)
        })?;
        tree_size(&owned.root)?;
        Ok(
            json!({"manifest":"normalized/manifest.json","manifestSha256":bound_file(&owned.root,"normalized/manifest.json")?.sha256,"normalization":"normalized/normalization.json","normalizationSha256":bound_file(&owned.root,"normalized/normalization.json")?.sha256,"charts":work.manifest["charts"].as_array().unwrap().len()}),
        )
    })();
    let (normalized, mut failure) = match result {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    if failure.is_none() {
        stage = "ready";
    }
    let mut manifest = json!({"schema":"ddi-danoni-native-plan-run/1","producer":"native-plan-orchestration/1","ready":failure.is_none(),"stage":stage,"request":plan,"worker":worker,"planning":"planning.json","planningSha256":bound_file(&owned.root,"planning.json")?.sha256,"captures":runs,"normalized":normalized,"failure":failure,"scope":"Explicit selected work captured and normalized; not URL discovery, musical tempo acceptance or library installation"});
    if failure.is_none() {
        // Serialize and write the complete receipt before the final cancellation
        // checkpoint. No further source reads/hashing can delay publication.
        write(&owned.root, ".manifest.json.tmp", &bytes(&manifest)?)?;
        if let Err(e) = progress(&Progress {
            stage: "ready",
            capture_id: None,
            completed: prepared.captures.len(),
            total: prepared.captures.len(),
        }) {
            fs::remove_file(owned.root.join(".manifest.json.tmp")).map_err(native::io)?;
            failure = Some(e);
            manifest["ready"] = json!(false);
            manifest["failure"] = json!(failure);
            manifest["normalized"] = Value::Null;
            atomic(&owned.root, "failure.json", &manifest)?;
        } else {
            fs::rename(
                owned.root.join(".manifest.json.tmp"),
                owned.root.join("manifest.json"),
            )
            .map_err(native::io)?;
        }
    } else {
        atomic(&owned.root, "failure.json", &manifest)?;
    }
    owned.retain = true;
    Ok(PlanOutcome {
        ready: failure.is_none(),
        output: owned.root.clone(),
        manifest,
        failure,
    })
}
pub fn load_job(path: &Path) -> Result<(PlanJob, DirectorySource)> {
    let path = path.canonicalize().map_err(native::io)?;
    native::external(&path)?;
    let parent = path.parent().unwrap();
    let source = DirectorySource::new(BTreeMap::from([("job".into(), parent.to_path_buf())]))?;
    let b = source.read(
        "job",
        path.file_name()
            .unwrap()
            .to_str()
            .ok_or_else(|| invalid("Job filename"))?,
        JSON_LIMIT,
    )?;
    let job: PlanJob = json(&b)?;
    if job.schema != "ddi-danoni-native-plan-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let roots = job
        .roots
        .iter()
        .map(|(k, p)| {
            (
                k.clone(),
                if p.is_absolute() {
                    p.clone()
                } else {
                    parent.join(p)
                },
            )
        })
        .collect();
    Ok((job, DirectorySource::new(roots)?))
}
pub fn execute_job(
    bundle: &Path,
    job: &Path,
    output: &Path,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<PlanOutcome> {
    let worker = crate::worker_build::load_bundle(bundle, &mut || {
        progress(&Progress {
            stage: "worker",
            capture_id: None,
            completed: 0,
            total: 1,
        })
    })?;
    let (job, source) = load_job(job)?;
    execute(&job.plan, &worker, &source, output, progress)
}
