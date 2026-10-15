#![cfg_attr(
    not(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64"))),
    allow(dead_code)
)]
//! Native host supervisor. A work never chooses the worker or confinement policy.
mod config;
#[path = "../../../ruffle/src/renderer_options.rs"]
pub mod renderer_options;
pub use renderer_options::RendererOptions;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux;
#[cfg(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64")))]
mod process;
use crate::{
    Error, Result,
    assets::{self, Inputs, valid_hash, *},
    native, swf,
};
use config::Config;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
const CONFIG_LIMIT: usize = 65536;
const WORKER_LIMIT: usize = 256 * 1024 * 1024;
#[cfg(not(target_os = "linux"))]
pub(crate) const PROFILE: &[u8] = include_bytes!("../../../ruffle/worker.sb");
#[cfg(target_os = "linux")]
pub(crate) const PROFILE: &[u8] = include_bytes!("linux.rs");
#[cfg(target_os = "linux")]
pub(crate) const POLICY_FILE: &str = "worker-linux.rs";
#[cfg(not(target_os = "linux"))]
pub(crate) const POLICY_FILE: &str = "worker.sb";
#[cfg(target_os = "linux")]
const PRODUCER: &str = "linux-landlock-seccomp-supervisor/1";
#[cfg(not(target_os = "linux"))]
const PRODUCER: &str = "macos-sandbox-supervisor/1";
pub(crate) const REVISION: &str = "89a7049965f3090e706265670df309a5128233ac";
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-native-capture", message)
}
/// This is installed/trusted host configuration, never decoded from a song page.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerSpec {
    pub executable: PathBuf,
    pub sha256: String,
    pub build_identity: PathBuf,
    pub build_identity_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Limits {
    pub wall_ms: u64,
    pub resident_mib: u64,
    pub cpu_seconds: u64,
    pub file_bytes: u64,
    pub output_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            wall_ms: 30000,
            resident_mib: 2048,
            cpu_seconds: 20,
            file_bytes: 40 * 1024 * 1024,
            output_bytes: 128 * 1024 * 1024,
        }
    }
}
impl Limits {
    pub(crate) fn validate(&self) -> Result<()> {
        if !(1..=60000).contains(&self.wall_ms)
            || !(1..=2048).contains(&self.resident_mib)
            || !(1..=20).contains(&self.cpu_seconds)
            || !(1024..=40 * 1024 * 1024).contains(&self.file_bytes)
            || !(1024..=256 * 1024 * 1024).contains(&self.output_bytes)
        {
            return Err(invalid(
                "Limits must stay inside the reviewed maximum budgets",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureRequest {
    pub config: Binding,
    pub movie: Binding,
    pub provenance: Option<Binding>,
    #[serde(default)]
    pub limits: Limits,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOutcome {
    pub ready: bool,
    pub output: PathBuf,
    pub run: Value,
    pub manifest: Option<Value>,
    pub failure: Option<Error>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub worker: WorkerSpec,
    pub request: CaptureRequest,
}
/// Identities of the tracked, reviewed hooks and worker template. The expected
/// binary hash is supplied by the host's installed package, not by this manifest.
pub fn expected_build_identity() -> Value {
    let files = reviewed_sources()
        .into_iter()
        .map(|(n, b)| (n, sha256(b)))
        .collect::<BTreeMap<_, _>>();
    json!({"release":"v0.7.1","revision":REVISION,"files":files})
}
pub(crate) fn reviewed_sources() -> BTreeMap<&'static str, &'static [u8]> {
    BTreeMap::from([
        (
            "src/renderer.rs",
            include_bytes!("../../../ruffle/src/renderer.rs").as_slice(),
        ),
        (
            "src/renderer_options.rs",
            include_bytes!("../../../ruffle/src/renderer_options.rs").as_slice(),
        ),
        (
            "prepare.py",
            include_bytes!("../../../ruffle/prepare.py").as_slice(),
        ),
        (
            "Cargo.toml.in",
            include_bytes!("../../../ruffle/Cargo.toml.in").as_slice(),
        ),
        (
            "Cargo.lock",
            include_bytes!("../../../ruffle/Cargo.lock").as_slice(),
        ),
        (
            "core-properties.rs",
            include_bytes!("../../../ruffle/core-properties.rs").as_slice(),
        ),
        (
            "core-snapshot.rs",
            include_bytes!("../../../ruffle/core-snapshot.rs").as_slice(),
        ),
        (
            "src/audio.rs",
            include_bytes!("../../../ruffle/src/audio.rs").as_slice(),
        ),
        (
            "src/main.rs",
            include_bytes!("../../../ruffle/src/main.rs").as_slice(),
        ),
        (
            "src/navigator.rs",
            include_bytes!("../../../ruffle/src/navigator.rs").as_slice(),
        ),
    ])
}
fn bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    let m = fs::symlink_metadata(path).map_err(native::io)?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > max as u64 {
        return Err(invalid("Need a bounded regular file, no symlinks"));
    }
    let mut b = vec![];
    fs::File::open(path)
        .map_err(native::io)?
        .take(max as u64 + 1)
        .read_to_end(&mut b)
        .map_err(native::io)?;
    if b.len() > max {
        return Err(invalid("File limit"));
    }
    Ok(b)
}
struct Plan {
    config: Config,
    config_value: Value,
    fps: f64,
    files: BTreeMap<String, Arc<Vec<u8>>>,
    source_inputs: BTreeMap<String, Binding>,
    executable: Vec<u8>,
    identity: Vec<u8>,
}
fn plan(
    request: &CaptureRequest,
    worker: &WorkerSpec,
    source: &impl AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Plan> {
    checkpoint()?;
    request.limits.validate()?;
    let mut inputs = Inputs::new(source);
    let config = inputs.checked(&request.config, CONFIG_LIMIT)?;
    let parsed = Config::parse(&config)?;
    #[cfg(target_os = "linux")]
    if parsed.render && parsed.render_scale.unwrap_or(1) > 1 {
        return Err(Error::new(
            "unsupported-native-render-scale",
            "Linux confined rendering is verified at native scale only; enlarged Lavapipe captures can exceed the 40 MiB kernel file budget",
        ));
    }
    let configured = request.config.sibling(&RelativeBinding {
        path: parsed.movie.clone(),
        sha256: request.movie.sha256.clone(),
    })?;
    if configured.namespace != request.movie.namespace || configured.path != request.movie.path {
        return Err(invalid("Configured movie differs from explicit binding"));
    }
    let movie = inputs.checked(&request.movie, swf::MOVIE_LIMIT)?;
    let fps = swf::Movie::read(&movie, checkpoint)?.header.fps;
    let mut files = BTreeMap::from([
        ("capture.json".into(), config.clone()),
        (parsed.movie.clone(), movie),
    ]);
    if files.len() != 2 {
        return Err(invalid("Movie path conflicts with staged config"));
    }
    let mut resource_bytes = 0;
    for r in &parsed.resources {
        checkpoint()?;
        let binding = request.config.sibling(&RelativeBinding {
            path: r.path.clone(),
            sha256: r.sha256.clone(),
        })?;
        let bytes = inputs.checked(&binding, 16 * 1024 * 1024)?;
        resource_bytes += bytes.len();
        if resource_bytes > 64 * 1024 * 1024 {
            return Err(invalid("Resource byte inventory limit"));
        }
        if let Some(previous) = files.insert(r.path.clone(), bytes.clone())
            && *previous != *bytes
        {
            return Err(invalid("Conflicting staged resource path"));
        }
    }
    if let Some(binding) = &request.provenance {
        let b = inputs.checked(binding, JSON_LIMIT)?;
        let provenance: Value = assets::json(&b)?;
        if provenance
            .get("sourceSwfSha256")
            .unwrap_or(&provenance["swf"]["sha256"])
            .as_str()
            != Some(request.movie.sha256.as_str())
        {
            return Err(invalid("Provenance belongs to another movie"));
        }
        if files.insert("source-manifest.json".into(), b).is_some() {
            return Err(invalid("Provenance path conflict"));
        }
    }
    if !valid_hash(&worker.sha256) || !valid_hash(&worker.build_identity_sha256) {
        return Err(invalid("Invalid trusted worker binding"));
    }
    let executable = bounded(&worker.executable, WORKER_LIMIT)?;
    let identity = bounded(&worker.build_identity, CONFIG_LIMIT)?;
    if sha256(&executable) != worker.sha256
        || sha256(&identity) != worker.build_identity_sha256
        || assets::json::<Value>(&identity)? != expected_build_identity()
    {
        return Err(Error::new(
            "unreviewed-native-worker",
            "Worker bytes or reviewed hook/build identity differ",
        ));
    }
    Ok(Plan {
        fps,
        config: parsed,
        config_value: assets::json(&config)?,
        files,
        source_inputs: inputs.used,
        executable,
        identity,
    })
}
struct OwnedOutput {
    path: PathBuf,
    retain: bool,
}
impl Drop for OwnedOutput {
    fn drop(&mut self) {
        if !self.retain {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn new_output(output: &Path) -> Result<OwnedOutput> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(native::io)?;
    native::external(&parent)?;
    let name = output
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("Missing portable output name"))?;
    relative_path(name)?;
    let path = parent.join(name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(native::io)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(&path).map_err(native::io)?;
    Ok(OwnedOutput {
        path,
        retain: false,
    })
}
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(native::io)?;
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(native::io)?;
    f.write_all(bytes).map_err(native::io)?;
    Ok(())
}
fn atomic_json(dir: &Path, name: &str, value: &Value) -> Result<()> {
    let temporary = dir.join(format!(".{name}.tmp"));
    write(
        &temporary,
        &serde_json::to_vec_pretty(value).map_err(|e| invalid(e.to_string()))?,
    )?;
    fs::rename(temporary, dir.join(name)).map_err(native::io)
}
/// Stage only bound inputs, launch under the fixed platform sandbox and retain a
/// success or failure receipt. Unsupported platforms never execute a worker.
pub fn capture(
    request: &CaptureRequest,
    worker: &WorkerSpec,
    source: &impl AssetSource,
    output: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<CaptureOutcome> {
    #[cfg(not(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64"))))]
    {
        let _ = (request, worker, source, output, checkpoint);
        Err(Error::new(
            "unsupported-native-confinement",
            "Native execution requires a supported macOS or Linux confinement backend",
        ))
    }
    #[cfg(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64")))]
    {
        #[cfg(target_os = "linux")]
        linux::available()?;
        let plan = plan(request, worker, source, checkpoint)?;
        let mut owned = new_output(output)?;
        let root = &owned.path;
        for (name, bytes) in &plan.files {
            checkpoint()?;
            write(&root.join("inputs").join(name), bytes)?;
        }
        write(&root.join("runtime/ddi-ruffle-probe"), &plan.executable)?;
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                root.join("runtime/ddi-ruffle-probe"),
                fs::Permissions::from_mode(0o500),
            )
            .map_err(native::io)?;
        }
        write(&root.join("runtime/build-identity.json"), &plan.identity)?;
        write(&root.join("runtime").join(POLICY_FILE), PROFILE)?;
        fs::create_dir(root.join("capture")).map_err(native::io)?;
        checkpoint()?;
        let mut run = process::run(root, &request.limits, checkpoint)?;
        let verification = verify_staged(root, &plan);
        let unchanged = matches!(verification, Ok(true));
        run["inputVerificationError"] = json!(verification.err());
        run["workerSha256"] = json!(worker.sha256);
        run["configSha256"] = json!(request.config.sha256);
        run["sandboxSha256"] = json!(sha256(PROFILE));
        run["inputsUnchanged"] = json!(unchanged);
        let mut failure = if run["exitCode"] != 0 || !run["termination"].is_null() {
            Some(Error::new(
                "native-capture-failed",
                format!(
                    "Restricted worker exit {} (termination {})",
                    run["exitCode"], run["termination"]
                ),
            ))
        } else {
            None
        };
        if !unchanged {
            run["termination"] = json!("inputs-changed-during-run");
            failure = Some(Error::new(
                "native-capture-inputs-changed",
                "Staged immutable files changed",
            ));
        }
        let artifact_result = inventory(root, &request.limits);
        if let Err(e) = &artifact_result {
            if run["termination"].is_null() {
                run["termination"] = json!("invalid-output");
            }
            failure = Some(e.clone());
        }
        if failure.is_none() {
            let mut cancelled = false;
            let validation = validate_capture(root, &plan, &mut || {
                let result = checkpoint();
                cancelled |= result.is_err();
                result
            });
            if let Err(e) = validation {
                run["termination"] = json!(if cancelled {
                    "cancelled"
                } else {
                    "invalid-capture"
                });
                failure = Some(e);
            }
        }
        atomic_json(root, "run.json", &run)?;
        let mut manifest = None;
        let bind = |name: &str| -> Result<Binding> {
            let bytes = bounded(&root.join(name), WORKER_LIMIT)?;
            Ok(Binding {
                namespace: "capture".into(),
                path: name.into(),
                sha256: sha256(&bytes),
            })
        };
        let mut bindings = if failure.is_none() {
            Some(json!({
                "capture": { "snapshot":bind("capture/snapshot.json")?, "run":bind("run.json")?, "config":bind("inputs/capture.json")? },
                "movie":bind(&format!("inputs/{}", plan.config.movie))?,
                "nativePcm":if plan.config.mix_audio {Some(bind("capture/audio.pcm")?)}else{None},
                "provenance":if request.provenance.is_some(){Some(bind("inputs/source-manifest.json")?)}else{None}
            }))
        } else {
            None
        };
        // A cancellation arriving after the child exits still prevents readiness.
        if failure.is_none()
            && let Err(e) = checkpoint()
        {
            run["termination"] = json!("cancelled");
            failure = Some(e);
            bindings = None;
            atomic_json(root, "run.json", &run)?;
        }
        let receipt = json!({"schema":"ddi-danoni-native-capture/1","producer":PRODUCER,"ready":failure.is_none(),"request":request,"worker":worker,"build":expected_build_identity(),"sandboxSha256":sha256(PROFILE),"sourceInputs":plan.source_inputs,"stagedInputs":plan.files.iter().map(|(path,bytes)|json!({"path":format!("inputs/{path}"),"sha256":sha256(bytes),"bytes":bytes.len()})).collect::<Vec<_>>(),"runtime":{"worker":"runtime/ddi-ruffle-probe","buildIdentity":"runtime/build-identity.json","sandbox":format!("runtime/{POLICY_FILE}")},"bindings":bindings,"run":run,"failure":failure,"artifacts":artifact_result.unwrap_or_default(),"scope":"Restricted native capture and transport validation; family, movement, audio-origin and original Flash equivalence remain separate proofs"});
        if failure.is_none() {
            atomic_json(root, "manifest.json", &receipt)?;
            manifest = Some(receipt);
        } else {
            atomic_json(root, "failure.json", &receipt)?;
        }
        owned.retain = true;
        Ok(CaptureOutcome {
            ready: failure.is_none(),
            output: root.clone(),
            run,
            manifest,
            failure,
        })
    }
}
fn verify_staged(root: &Path, plan: &Plan) -> Result<bool> {
    for (name, b) in &plan.files {
        if bounded(&root.join("inputs").join(name), b.len())? != **b {
            return Ok(false);
        }
    }
    Ok(
        bounded(&root.join("runtime/ddi-ruffle-probe"), WORKER_LIMIT)? == plan.executable
            && bounded(&root.join("runtime/build-identity.json"), CONFIG_LIMIT)? == plan.identity
            && bounded(&root.join("runtime").join(POLICY_FILE), CONFIG_LIMIT)? == PROFILE,
    )
}
fn inventory(root: &Path, limits: &Limits) -> Result<Vec<Value>> {
    let mut out = vec![];
    let mut total = 0u64;
    for entry in fs::read_dir(root.join("capture")).map_err(native::io)? {
        let path = entry.map_err(native::io)?.path();
        let m = fs::symlink_metadata(&path).map_err(native::io)?;
        if !m.is_file() || m.file_type().is_symlink() || m.len() > limits.file_bytes {
            return Err(Error::new(
                "invalid-native-output",
                "Nonregular or oversized worker output",
            ));
        }
        total = total
            .checked_add(m.len())
            .ok_or_else(|| invalid("Output size overflow"))?;
        if total > limits.output_bytes || out.len() >= 32 {
            return Err(Error::new(
                "native-output-limit",
                "Worker output byte/file limit",
            ));
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| invalid("Nonportable output name"))?;
        let b = bounded(&path, limits.file_bytes as usize)?;
        out.push(json!({"path":format!("capture/{name}"),"sha256":sha256(&b),"bytes":b.len()}));
    }
    out.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(out)
}
fn validate_capture(
    root: &Path,
    plan: &Plan,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    let bytes = bounded(&root.join("capture/snapshot.json"), 32 * 1024 * 1024)?;
    let s: Value = assets::json(&bytes)?;
    if s["schema"] != "ddi-ruffle-probe/experimental-1"
        || s["build"] != expected_build_identity()
        || s["ruffleRevision"] != REVISION
        || s["ruffleRelease"] != "v0.7.1"
        || s["source"]["config"] != plan.config_value
        || s["source"]["movieSha256"] != sha256(&plan.files[&plan.config.movie])
        || s["avm1Halted"] != false
        || s["rendered"] != plan.config.render
    {
        return Err(Error::new(
            "native-capture-contract",
            "Worker protocol, source, build, settings or halted state differ",
        ));
    }
    if plan.config.render {
        plan.config
            .renderer
            .clone()
            .unwrap_or_default()
            .validate_report(&s["renderer"])
            .map_err(invalid)?;
        let renderer: Value =
            assets::json(&bounded(&root.join("capture/renderer.json"), 1024 * 1024)?)?;
        if renderer != s["renderer"] {
            return Err(invalid("Renderer report differs from snapshot"));
        }
    } else if !s["renderer"].is_null() {
        return Err(invalid("Unexpected renderer on a renderer-free capture"));
    }
    if s["swfHeaderFps"].as_f64() != Some(plan.fps)
        || s["renderScale"] != plan.config.render_scale.unwrap_or(1)
    {
        return Err(invalid(
            "Movie FPS or render scale differs from the bound source",
        ));
    }
    let captures = s["captures"]
        .as_array()
        .ok_or_else(|| invalid("Missing captures"))?;
    if captures.len() != plan.config.capture_frames.len() {
        return Err(invalid("Capture coverage differs"));
    }
    for (item, frame) in captures.iter().zip(&plan.config.capture_frames) {
        checkpoint()?;
        if item["frame"] != *frame
            || item["state"]["incomplete"] != false
            || item["state"]["avm1Halted"] != false
        {
            return Err(Error::new(
                "native-capture-incomplete",
                "Incomplete or unexpected capture iteration",
            ));
        }
        crate::extraction::Graph::new(&item["state"])?;
        if plan.config.inspect_globals && !item["state"]["globalScopes"].is_array() {
            return Err(invalid("Missing requested globals"));
        }
    }
    let audit = &s["audit"];
    if audit["truncated"] != false || audit["droppedEntries"] != 0 || audit["truncatedDetails"] != 0
    {
        return Err(Error::new(
            "native-capture-incomplete",
            "Incomplete runtime audit",
        ));
    }
    let entries = audit["entries"]
        .as_array()
        .ok_or_else(|| invalid("Missing audit"))?;
    if entries.len() > 100000
        || audit["runtimeErrors"].as_u64()
            != Some(
                entries
                    .iter()
                    .filter(|e| e["kind"] == "runtime-error")
                    .count() as u64,
            )
    {
        return Err(invalid("Audit count mismatch"));
    }
    let fps = s["swfHeaderFps"]
        .as_f64()
        .filter(|f| (1.0..=240.0).contains(f))
        .ok_or_else(|| invalid("Invalid source FPS"))?;
    let mut expected =
        std::collections::BTreeSet::from(["snapshot.json".to_string(), "stderr.txt".into()]);
    if plan.config.mix_audio {
        let audio = &s["audio"];
        let frames =
            (*plan.config.capture_frames.last().unwrap() as f64 * 44100.0 / fps).floor() as u64;
        if audio["sampleRate"] != 44100
            || audio["channels"] != 2
            || audio["sampleFrames"].as_u64() != Some(frames)
            || fs::metadata(root.join("capture/audio.pcm"))
                .map_err(native::io)?
                .len()
                != frames * 4
        {
            return Err(invalid("Native PCM dimensions differ"));
        }
        expected.insert("audio.pcm".into());
    } else if !s["audio"].is_null() {
        return Err(invalid("Unexpected mixed audio"));
    }
    if plan.config.render {
        expected.insert("renderer.json".into());
        let scale = plan.config.render_scale.unwrap_or(1);
        let header = format!("P6\n{} {}\n255\n", 320 * scale, 240 * scale);
        if s["renderScale"] != scale {
            return Err(invalid("Render scale differs"));
        }
        for frame in &plan.config.capture_frames {
            checkpoint()?;
            let name = format!("frame-{frame}.ppm");
            let b = bounded(&root.join("capture").join(&name), limits_ppm(scale))?;
            if !b.starts_with(header.as_bytes())
                || b.len() != header.len() + (320 * 240 * 3 * scale * scale) as usize
            {
                return Err(invalid("Rendered pixel dimensions differ"));
            }
            expected.insert(name);
        }
    }
    for e in fs::read_dir(root.join("capture")).map_err(native::io)? {
        let e = e.map_err(native::io)?;
        if !expected.remove(
            e.file_name()
                .to_str()
                .ok_or_else(|| invalid("Nonportable output"))?,
        ) {
            return Err(invalid("Unexpected worker output"));
        }
    }
    if !expected.is_empty() {
        return Err(invalid("Missing worker output"));
    }
    checkpoint()?;
    Ok(())
}
fn limits_ppm(scale: u64) -> usize {
    (320 * 240 * 3 * scale * scale) as usize + 64
}
/// Explicit external JSON adapter; relative paths resolve against the job.
pub fn capture_job(
    job: &Path,
    output: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<CaptureOutcome> {
    let path = job.canonicalize().map_err(native::io)?;
    native::external(&path)?;
    let mut job: CaptureJob = assets::json(&bounded(&path, JSON_LIMIT)?)?;
    if job.schema != "ddi-danoni-native-capture-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let parent = path.parent().unwrap();
    for p in job
        .roots
        .values_mut()
        .chain([&mut job.worker.executable, &mut job.worker.build_identity])
    {
        if !p.is_absolute() {
            *p = parent.join(&*p);
        }
    }
    let source = native::DirectorySource::new(job.roots)?;
    capture(&job.request, &job.worker, &source, output, checkpoint)
}

/// Internal shared validator; orchestration does not loosen worker inputs.
pub(crate) fn validate_config(bytes: &[u8]) -> Result<()> {
    if bytes.len() > CONFIG_LIMIT {
        return Err(invalid("Configuration exceeds 64 KiB"));
    }
    Config::parse(bytes).map(|_| ())
}
