//! Bounded native dependency closure. The host resolves approved cached GETs;
//! the worker has no network, and only complete chart/audio proofs yield ready.
use crate::{
    Error, Result,
    assets::*,
    native::{self, DirectorySource},
    orchestration::{self, NativePlan, PlanJob},
    supervisor::{self, WorkerSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use url::Url;
mod workflow;
use workflow::{Attempt, Workflow};
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-native-dependency", message)
}
fn limited(message: impl Into<String>) -> Error {
    Error::new("native-dependency-limit", message)
}
fn bytes(v: &impl Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(v).map_err(|e| invalid(e.to_string()))
}
fn address(s: &str) -> Result<Url> {
    if s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control) {
        return Err(invalid("Invalid dependency URL"));
    }
    let u = Url::parse(s).map_err(|e| invalid(e.to_string()))?;
    if !matches!(u.scheme(), "http" | "https")
        || !u.has_host()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
    {
        return Err(invalid(
            "Need an anonymous HTTP(S) dependency without a fragment",
        ));
    }
    Ok(u)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct DependencyLimits {
    pub attempts: usize,
    pub captures: usize,
    pub new_urls: usize,
    pub new_bytes: u64,
    pub wall_ms: u64,
    pub output_bytes: u64,
}
impl Default for DependencyLimits {
    fn default() -> Self {
        Self {
            attempts: 4,
            captures: 32,
            new_urls: 16,
            new_bytes: 32 * 1024 * 1024,
            wall_ms: 120000,
            output_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}
impl DependencyLimits {
    pub(crate) fn validate(&self) -> Result<()> {
        if !(1..=8).contains(&self.attempts)
            || !(1..=64).contains(&self.captures)
            || !(1..=64).contains(&self.new_urls)
            || !(1..=64 * 1024 * 1024).contains(&self.new_bytes)
            || !(1..=300000).contains(&self.wall_ms)
            || !(1024..=2 * 1024 * 1024 * 1024).contains(&self.output_bytes)
        {
            return Err(limited("Dependency budgets exceed reviewed maxima"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestRecord {
    schema: String,
    method: String,
    has_body: bool,
    has_headers: bool,
    original_url: String,
    resolved_url: Option<String>,
    reason: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestObservation {
    pub iteration: u64,
    pub original_url: String,
    pub url: String,
}
/// Passive inspection only. `execute` additionally binds these bytes to the
/// successful supervised run/config/movie; this value alone is no fetch authority.
pub fn inspect_requests(capture: &Value) -> Result<Vec<RequestObservation>> {
    if capture["schema"] != "ddi-ruffle-probe/experimental-1"
        || capture["build"] != supervisor::expected_build_identity()
        || capture["ruffleRevision"] != supervisor::REVISION
        || capture["avm1Halted"] != false
    {
        return Err(invalid("Need current complete worker request evidence"));
    }
    let config = &capture["source"]["config"];
    supervisor::validate_config(&bytes(config)?)?;
    let base = address(
        config["sourceUrl"]
            .as_str()
            .ok_or_else(|| invalid("Missing source URL"))?,
    )?;
    let max_frame = config["captureFrames"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_u64()
        .unwrap();
    let mut supplied = BTreeSet::new();
    if let Some(rows) = config["resources"].as_array() {
        for r in rows {
            supplied.insert(address(r["url"].as_str().unwrap())?.to_string());
        }
    }
    let audit = &capture["audit"];
    if audit["truncated"] != false || audit["droppedEntries"] != 0 || audit["truncatedDetails"] != 0
    {
        return Err(invalid("Incomplete request audit"));
    }
    let entries = audit["entries"]
        .as_array()
        .filter(|a| a.len() <= 256)
        .ok_or_else(|| invalid("Invalid request audit entries"))?;
    if audit["runtimeErrors"].as_u64()
        != Some(
            entries
                .iter()
                .filter(|e| e["kind"] == "runtime-error")
                .count() as u64,
        )
    {
        return Err(invalid("Runtime-error count differs"));
    }
    let mut result = Vec::new();
    for entry in entries {
        if entry["truncated"] != false {
            return Err(invalid("Truncated request entry"));
        }
        let iteration = entry["iteration"]
            .as_u64()
            .filter(|i| *i <= max_frame)
            .ok_or_else(|| invalid("Request iteration outside capture"))?;
        match entry["kind"].as_str() {
            Some("runtime-warning" | "runtime-error") => {} // Never treated as requests; final normalization still checks them.
            Some("cache-hit")
                if entry["detail"]
                    .as_str()
                    .is_some_and(|s| supplied.contains(s)) => {}
            Some("fetch-denied") => {
                let request: RequestRecord = serde_json::from_value(entry["request"].clone())
                    .map_err(|_| invalid("URL-only or invalid denied-request evidence"))?;
                if request.schema != "ddi-native-request/1"
                    || request.method != "GET"
                    || request.has_body
                    || request.has_headers
                    || request.reason != "cache-miss"
                {
                    return Err(invalid(
                        "Only exact body/header-free GET cache misses can be dependencies",
                    ));
                }
                if entry["detail"] != request.original_url
                    || request.original_url.len() > 4096
                    || request.original_url.chars().any(char::is_control)
                {
                    return Err(invalid("Original request identity differs"));
                }
                let resolved = base
                    .join(&request.original_url)
                    .map_err(|e| invalid(e.to_string()))?;
                let url = address(
                    request
                        .resolved_url
                        .as_deref()
                        .ok_or_else(|| invalid("Missing resolved URL"))?,
                )?;
                if resolved != url || request.resolved_url.as_deref() != Some(url.as_str()) {
                    return Err(invalid("Logical URL resolution differs"));
                }
                if supplied.contains(url.as_str()) {
                    return Err(Error::new(
                        "native-dependency-no-progress",
                        "Worker requested a dependency already declared in this configuration",
                    ));
                }
                result.push(RequestObservation {
                    iteration,
                    original_url: request.original_url,
                    url: url.to_string(),
                });
            }
            _ => {
                return Err(invalid(
                    "Navigation, socket, trace or unknown audit entry is not a dependency",
                ));
            }
        }
    }
    Ok(result)
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestWitness {
    pub capture_id: String,
    pub chart_ids: Vec<String>,
    pub iteration: u64,
    pub original_url: String,
    pub snapshot: Binding,
    pub source_url: String,
    pub page_url: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingDependency {
    pub url: String,
    /// The host must apply this limit while receiving a response, not only after.
    pub maximum_bytes: u64,
    pub witnesses: Vec<RequestWitness>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DependencyResponse {
    pub body: Binding,
    pub provenance: Binding,
    pub http_charset: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResponseProvenance {
    pub schema: String,
    pub logical_url: String,
    pub retrieval_url: String,
    pub retrieved_at: String,
    pub status: u16,
    pub sha256: String,
    pub policy_decision: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: &'static str,
    pub attempt: usize,
    pub captures: usize,
    pub new_urls: usize,
    pub new_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyOutcome {
    pub ready: bool,
    pub output: PathBuf,
    pub manifest: Value,
    pub failure: Option<Error>,
}
struct Memory(BTreeMap<String, Arc<Vec<u8>>>);
impl AssetSource for Memory {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        relative_path(path)?;
        if !["plan", "profile"].contains(&ns) {
            return Err(invalid("Unknown dependency-stage namespace"));
        }
        let b = self
            .0
            .get(path)
            .ok_or_else(|| invalid("Missing dependency-stage input"))?;
        if b.len() > max {
            return Err(limited("Dependency-stage file limit"));
        }
        Ok(b.as_ref().clone())
    }
}
fn bind(path: String, b: &[u8]) -> Binding {
    Binding {
        namespace: "plan".into(),
        path,
        sha256: sha256(b),
    }
}
fn file(source: &DirectorySource, path: &str, max: usize) -> Result<Vec<u8>> {
    source.read("attempt", path, max)
}
fn requests_from_attempt(
    outcome: &Attempt,
    worker: &WorkerSpec,
    attempt: &str,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<MissingDependency>> {
    if outcome.manifest["stage"] != "normalization" {
        return Err(outcome
            .failure
            .clone()
            .unwrap_or_else(|| invalid("Capture stage did not complete")));
    }
    let source =
        DirectorySource::new(BTreeMap::from([("attempt".into(), outcome.output.clone())]))?;
    let planning_bytes = file(&source, "planning.json", JSON_LIMIT)?;
    if sha256(&planning_bytes) != outcome.manifest["planningSha256"] {
        return Err(invalid("Plan receipt hash differs"));
    }
    let planning: Value = json(&planning_bytes)?;
    let captures = outcome.manifest["captures"]
        .as_object()
        .ok_or_else(|| invalid("Missing completed captures"))?;
    let mut requests: BTreeMap<String, MissingDependency> = BTreeMap::new();
    for (id, row) in captures {
        checkpoint()?;
        relative_path(id)?;
        if row["ready"] != true {
            return Err(invalid("Unsuccessful transport cannot supply requests"));
        }
        let child_bytes = file(&source, &format!("captures/{id}/manifest.json"), JSON_LIMIT)?;
        if sha256(&child_bytes) != row["manifestSha256"] {
            return Err(invalid("Capture receipt hash differs"));
        }
        let child: Value = json(&child_bytes)?;
        if child["ready"] != true
            || child["build"] != supervisor::expected_build_identity()
            || child["worker"]
                != serde_json::to_value(worker).map_err(|e| invalid(e.to_string()))?
        {
            return Err(invalid("Capture worker identity differs"));
        }
        let snapshot_path = format!("captures/{id}/capture/snapshot.json");
        let snapshot_bytes = file(&source, &snapshot_path, 32 * 1024 * 1024)?;
        let snapshot: Value = json(&snapshot_bytes)?;
        let run_bytes = file(&source, &format!("captures/{id}/run.json"), JSON_LIMIT)?;
        let run: Value = json(&run_bytes)?;
        let config_bytes = file(
            &source,
            &format!("captures/{id}/inputs/capture.json"),
            65536,
        )?;
        let config: Value = json(&config_bytes)?;
        if sha256(&snapshot_bytes) != child["bindings"]["capture"]["snapshot"]["sha256"]
            || sha256(&run_bytes) != child["bindings"]["capture"]["run"]["sha256"]
            || sha256(&config_bytes) != run["configSha256"]
            || run != child["run"]
            || run != row["run"]
            || run["inputsUnchanged"] != true
            || run["exitCode"] != 0
            || !run["termination"].is_null()
            || run["workerSha256"] != worker.sha256
            || snapshot["source"]["config"] != config
            || snapshot["source"]["movieSha256"] != outcome.manifest["request"]["movie"]["sha256"]
        {
            return Err(invalid("Request capture/config/run binding differs"));
        }
        let ids = planning["charts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| {
                ["projection", "motion", "audio"]
                    .iter()
                    .any(|k| c[*k] == *id)
            })
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Err(invalid("Unmapped dependency capture"));
        }
        for observation in inspect_requests(&snapshot)? {
            requests
                .entry(observation.url.clone())
                .or_insert_with(|| MissingDependency {
                    url: observation.url,
                    maximum_bytes: 16 * 1024 * 1024,
                    witnesses: vec![],
                })
                .witnesses
                .push(RequestWitness {
                    capture_id: id.clone(),
                    chart_ids: ids.clone(),
                    iteration: observation.iteration,
                    original_url: observation.original_url,
                    snapshot: Binding {
                        namespace: "dependency".into(),
                        path: format!("{attempt}/{snapshot_path}"),
                        sha256: sha256(&snapshot_bytes),
                    },
                    source_url: config["sourceUrl"].as_str().unwrap().into(),
                    page_url: config["pageUrl"].as_str().map(str::to_string),
                });
        }
    }
    Ok(requests.into_values().collect())
}
#[derive(Clone)]
struct Accepted {
    resource: Value,
}
fn accept(
    request: &MissingDependency,
    response: &DependencyResponse,
    inputs: &mut Inputs<'_, impl AssetSource>,
    memory: &mut Memory,
    root: &Path,
) -> Result<(Accepted, Value, u64)> {
    if let Some(label) = &response.http_charset
        && encoding_rs::Encoding::for_label(label.as_bytes()).is_none()
    {
        return Err(invalid("Unknown dependency HTTP charset"));
    }
    let provenance_bytes = inputs.checked(&response.provenance, 65536)?;
    let provenance: ResponseProvenance = json(&provenance_bytes)?;
    if provenance.schema != "ddi-danoni-dependency-response/1"
        || provenance.logical_url != request.url
        || provenance.sha256 != response.body.sha256
        || provenance.retrieved_at.is_empty()
        || provenance.retrieved_at.len() > 128
        || provenance.retrieved_at.chars().any(char::is_control)
        || provenance.policy_decision.trim().is_empty()
        || provenance.policy_decision.len() > 4096
    {
        return Err(invalid(
            "Dependency response provenance differs or lacks host policy decision",
        ));
    }
    address(&provenance.retrieval_url)?;
    if provenance.status != 200 {
        return Err(Error::new(
            "native-dependency-unavailable",
            format!("Dependency returned HTTP {}", provenance.status),
        ));
    }
    let body = inputs.checked(&response.body, 16 * 1024 * 1024)?;
    let hash = sha256(&body);
    let resource_path = format!("inputs/blobs/{hash}");
    let folder = format!("dependencies/{}", sha256(request.url.as_bytes()));
    orchestration::write(root, &format!("{folder}/body"), &body)?;
    orchestration::write(root, &format!("{folder}/response.json"), &provenance_bytes)?;
    memory.0.insert(resource_path.clone(), body.clone());
    Ok((
        Accepted {
            resource: json!({"url":request.url,"path":format!("blobs/{hash}"),"sha256":hash,"httpCharset":response.http_charset}),
        },
        json!({"request":request,"source":response,"provenance":provenance,"body":{"path":format!("{folder}/body"),"sha256":hash,"bytes":body.len()},"receipt":{"path":format!("{folder}/response.json"),"sha256":sha256(&provenance_bytes)}}),
        body.len() as u64,
    ))
}
fn reserve(root: &Path, extra: u64, limits: &DependencyLimits) -> Result<()> {
    if orchestration::tree_size(root)?
        .checked_add(extra)
        .is_none_or(|n| n > limits.output_bytes)
    {
        return Err(limited(
            "Dependency output reservation exceeds operation budget",
        ));
    }
    Ok(())
}
/// Resolve must use host policy/cache services and honor its checkpoint while
/// waiting. It returns bound bytes through `source`; no script is evaluated here.
/// Source failures occur before output creation; later failure/cancellation
/// retains diagnostics without a parent ready marker. Nothing installs a song.
pub fn execute(
    plan: &NativePlan,
    worker: &WorkerSpec,
    source: &impl AssetSource,
    output: &Path,
    limits: &DependencyLimits,
    resolve: &mut impl FnMut(
        &MissingDependency,
        &mut dyn FnMut() -> Result<()>,
    ) -> Result<DependencyResponse>,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<DependencyOutcome> {
    execute_workflow(
        &Workflow::Plan(Box::new(plan.clone())),
        worker,
        source,
        output,
        limits,
        resolve,
        progress,
    )
}
/// Automatic menu discovery and dependency closure share the explicit-plan
/// coordinator's host resolver, response validation and whole-operation budgets.
pub fn execute_profile(
    request: &crate::profiles::ProfileRequest,
    worker: &WorkerSpec,
    source: &impl AssetSource,
    output: &Path,
    limits: &DependencyLimits,
    resolve: &mut impl FnMut(
        &MissingDependency,
        &mut dyn FnMut() -> Result<()>,
    ) -> Result<DependencyResponse>,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<DependencyOutcome> {
    execute_workflow(
        &Workflow::Profile(Box::new(request.clone())),
        worker,
        source,
        output,
        limits,
        resolve,
        progress,
    )
}
fn execute_workflow(
    original: &Workflow,
    worker: &WorkerSpec,
    source: &impl AssetSource,
    output: &Path,
    limits: &DependencyLimits,
    resolve: &mut impl FnMut(
        &MissingDependency,
        &mut dyn FnMut() -> Result<()>,
    ) -> Result<DependencyResponse>,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<DependencyOutcome> {
    limits.validate()?;
    let started = Instant::now();
    let mut state = Progress {
        stage: "planning",
        attempt: 0,
        captures: 0,
        new_urls: 0,
        new_bytes: 0,
    };
    let mut last_output_check = Instant::now();
    let mut check = |state: &Progress, root: Option<&Path>| -> Result<()> {
        if started.elapsed().as_millis() > limits.wall_ms as u128 {
            return Err(limited("Total dependency wall budget exhausted"));
        }
        // CPU-side decoders/checkers call checkpoints very frequently. Poll
        // disk use at 50 ms, with forced checks at readiness boundaries; full
        // reservations below precede every capture and normalization phase.
        if let Some(root) = root
            && (last_output_check.elapsed() >= Duration::from_millis(50)
                || matches!(state.stage, "ready" | "attempt-ready"))
        {
            reserve(root, 0, limits)?;
            last_output_check = Instant::now();
        }
        progress(state)?;
        if started.elapsed().as_millis() > limits.wall_ms as u128 {
            return Err(limited(
                "Total dependency wall budget exhausted after host callback",
            ));
        }
        Ok(())
    };
    check(&state, None)?;
    let (mut current, mut memory) = original.seed(source, &mut || check(&state, None))?;
    let runtime_bytes = orchestration::worker_bytes(worker)?;
    let mut owned = orchestration::new_root(output)?;
    for (path, b) in &memory.0 {
        check(&state, Some(&owned.root))?;
        reserve(&owned.root, b.len() as u64, limits)?;
        orchestration::write(&owned.root, &format!("seed/{path}"), b)?;
    }
    fs::create_dir(owned.root.join("attempts")).map_err(native::io)?;
    fs::create_dir(owned.root.join("requests")).map_err(native::io)?;
    let mut accepted: BTreeMap<String, Accepted> = BTreeMap::new();
    let mut responses = BTreeMap::new();
    let mut response_attempts = Vec::new();
    let mut attempts = Vec::new();
    let mut response_inputs = Inputs::new(source);
    let result = (|| -> Result<Value> {
        for attempt in 0..limits.attempts {
            state.attempt = attempt;
            state.stage = "planning";
            check(&state, Some(&owned.root))?;
            let (input_bytes, known_captures) =
                current.budget(&memory, &mut || check(&state, Some(&owned.root)))?;
            let capture_base = state.captures;
            if let Some(count) = known_captures {
                state.captures = state
                    .captures
                    .checked_add(count)
                    .ok_or_else(|| limited("Capture count overflow"))?;
                if state.captures > limits.captures {
                    return Err(limited("Total dependency capture budget exhausted"));
                }
            } else if capture_base >= limits.captures {
                return Err(limited("Total dependency capture budget exhausted"));
            }
            reserve(&owned.root, input_bytes + 2 * 1024 * 1024, limits)?;
            let mut reserved_captures = BTreeSet::new();
            let mut normalization_reserved = false;
            let path = format!("attempts/{attempt:03}");
            let outcome = current.run(
                worker,
                &memory,
                &owned.root.join(&path),
                limits.captures.saturating_sub(capture_base),
                limits
                    .wall_ms
                    .saturating_sub(started.elapsed().as_millis() as u64),
                &mut |p| {
                    let new_capture = if known_captures.is_some() {
                        p.stage == "capture"
                            && p.capture_id
                                .is_some_and(|id| reserved_captures.insert(id.to_string()))
                    } else {
                        let count = capture_base
                            .checked_add(p.captures)
                            .ok_or_else(|| limited("Capture count overflow"))?;
                        if count > limits.captures {
                            return Err(limited("Total dependency capture budget exhausted"));
                        }
                        let changed = count > state.captures;
                        state.captures = count;
                        changed
                    };
                    if new_capture {
                        reserve(
                            &owned.root,
                            input_bytes
                                + runtime_bytes
                                + current.capture_bytes()
                                + 578 * 1024 * 1024,
                            limits,
                        )?;
                    }
                    if p.stage == "normalization" && !normalization_reserved {
                        reserve(&owned.root, 576 * 1024 * 1024, limits)?;
                        normalization_reserved = true;
                    }
                    state.stage = if p.stage == "ready" {
                        "attempt-ready"
                    } else {
                        p.stage
                    };
                    check(&state, Some(&owned.root))
                },
            )?;
            let receipt_path = format!(
                "{path}/{}",
                if outcome.ready {
                    "manifest.json"
                } else {
                    "failure.json"
                }
            );
            let receipt = fs::read(owned.root.join(&receipt_path)).map_err(native::io)?;
            attempts.push(json!({"attempt":attempt,"ready":outcome.ready,"receipt":{"path":receipt_path,"sha256":sha256(&receipt)},"failure":outcome.failure}));
            check(&state, Some(&owned.root))?;
            if !outcome.ready
                && known_captures.is_none()
                && state.captures >= limits.captures
                && outcome
                    .failure
                    .as_ref()
                    .is_some_and(|e| e.code == "native-profile-limit")
            {
                return Err(limited(
                    "Total dependency capture budget exhausted inside profile",
                ));
            }
            if outcome.ready {
                let mut normalized = outcome.manifest["normalized"].clone();
                for name in ["manifest", "normalization"] {
                    normalized[name] =
                        json!(format!("{path}/{}", normalized[name].as_str().unwrap()));
                }
                return Ok(
                    json!({"attempt":attempt,"receipt":{"path":receipt_path,"sha256":sha256(&receipt)},"normalized":normalized}),
                );
            }
            let mut requests = current.requests(&outcome, worker, &path, &memory, &mut || {
                check(&state, Some(&owned.root))
            })?;
            for request in &mut requests {
                request.maximum_bytes = (limits.new_bytes - state.new_bytes).min(16 * 1024 * 1024);
            }
            orchestration::atomic(
                &owned.root,
                &format!("requests/{attempt:03}.json"),
                &json!({"schema":"ddi-danoni-missing-dependencies/1","attempt":attempt,"requests":requests}),
            )?;
            if requests.is_empty() {
                return Err(outcome.failure.unwrap_or_else(|| {
                    invalid("No missing dependency explains failed extraction")
                }));
            }
            if attempt + 1 >= limits.attempts {
                return Err(limited("Dependency attempt budget exhausted"));
            }
            for mut request in requests {
                request.maximum_bytes = (limits.new_bytes - state.new_bytes).min(16 * 1024 * 1024);
                state.stage = "dependency";
                check(&state, Some(&owned.root))?;
                if !accepted.contains_key(&request.url) {
                    if state.new_urls >= limits.new_urls {
                        return Err(limited("New dependency URL budget exhausted"));
                    }
                    let response = resolve(&request, &mut || check(&state, Some(&owned.root)))?;
                    check(&state, Some(&owned.root))?;
                    response_attempts
                        .push(json!({"attempt":attempt,"url":request.url,"response":response}));
                    // Read once into the input cache before mutating staged output.
                    let b = response_inputs.checked(&response.body, 16 * 1024 * 1024)?;
                    if state
                        .new_bytes
                        .checked_add(b.len() as u64)
                        .is_none_or(|v| v > limits.new_bytes)
                    {
                        return Err(limited("New dependency byte budget exhausted"));
                    }
                    reserve(&owned.root, b.len() as u64 + 65536, limits)?;
                    let (value, record, count) = accept(
                        &request,
                        &response,
                        &mut response_inputs,
                        &mut memory,
                        &owned.root,
                    )?;
                    state.new_urls += 1;
                    state.new_bytes += count;
                    responses.insert(request.url.clone(), record);
                    accepted.insert(request.url.clone(), value);
                    check(&state, Some(&owned.root))?;
                }
                let value = &accepted[&request.url];
                current.append(&request, value, &mut memory, attempt)?;
            }
        }
        Err(limited("Dependency attempt budget exhausted"))
    })();
    let (completed, mut failure) = match result {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    let mut manifest = json!({"schema":original.schema(),"producer":original.producer(),"ready":failure.is_none(),"request":original.request()?,"limits":limits,"worker":worker,"attempts":attempts,"responses":responses,"responseAttempts":response_attempts,"responseInputs":response_inputs.used,"counts":{"captures":state.captures,"newUrls":state.new_urls,"newBytes":state.new_bytes},"completed":completed,"failure":failure,"scope":"Host-approved GET dependency closure; no built-in HTTP client, tempo acceptance or installation"});
    if failure.is_none() {
        orchestration::write(&owned.root, ".manifest.json.tmp", &bytes(&manifest)?)?;
        state.stage = "ready";
        if let Err(e) = check(&state, Some(&owned.root)) {
            fs::remove_file(owned.root.join(".manifest.json.tmp")).map_err(native::io)?;
            failure = Some(e);
            manifest["ready"] = json!(false);
            manifest["completed"] = Value::Null;
            manifest["failure"] = json!(failure);
        } else {
            fs::rename(
                owned.root.join(".manifest.json.tmp"),
                owned.root.join("manifest.json"),
            )
            .map_err(native::io)?;
        }
    }
    if failure.is_some() {
        orchestration::atomic(&owned.root, "failure.json", &manifest)?;
    }
    owned.retain = true;
    Ok(DependencyOutcome {
        ready: failure.is_none(),
        output: owned.root.clone(),
        manifest,
        failure,
    })
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DependencyJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub plan: NativePlan,
    #[serde(default)]
    pub limits: DependencyLimits,
    #[serde(default)]
    pub responses: BTreeMap<String, DependencyResponse>,
}
pub fn execute_job(
    bundle: &Path,
    job: &Path,
    output: &Path,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<DependencyOutcome> {
    let worker = crate::worker_build::load_bundle(bundle, &mut || {
        progress(&Progress {
            stage: "worker",
            attempt: 0,
            captures: 0,
            new_urls: 0,
            new_bytes: 0,
        })
    })?;
    let path = job.canonicalize().map_err(native::io)?;
    native::external(&path)?;
    let parent = path.parent().unwrap();
    let jobs = DirectorySource::new(BTreeMap::from([("job".into(), parent.to_path_buf())]))?;
    let job: DependencyJob = json(
        &jobs.read(
            "job",
            path.file_name()
                .unwrap()
                .to_str()
                .ok_or_else(|| invalid("Job filename"))?,
            JSON_LIMIT,
        )?,
    )?;
    if job.schema != "ddi-danoni-native-dependency-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let source = DirectorySource::new(
        job.roots
            .into_iter()
            .map(|(k, p)| (k, if p.is_absolute() { p } else { parent.join(p) }))
            .collect(),
    )?;
    execute(
        &job.plan,
        &worker,
        &source,
        output,
        &job.limits,
        &mut |request, checkpoint| {
            checkpoint()?;
            job.responses.get(&request.url).cloned().ok_or_else(|| {
                Error::new(
                    "native-dependency-unavailable",
                    format!("No host-approved cached response for {}", request.url),
                )
            })
        },
        progress,
    )
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileDependencyJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub request: crate::profiles::ProfileRequest,
    #[serde(default)]
    pub limits: DependencyLimits,
    #[serde(default)]
    pub responses: BTreeMap<String, DependencyResponse>,
}
pub fn execute_profile_job(
    bundle: &Path,
    job: &Path,
    output: &Path,
    progress: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<DependencyOutcome> {
    let worker = crate::worker_build::load_bundle(bundle, &mut || {
        progress(&Progress {
            stage: "worker",
            attempt: 0,
            captures: 0,
            new_urls: 0,
            new_bytes: 0,
        })
    })?;
    let path = job.canonicalize().map_err(native::io)?;
    native::external(&path)?;
    let parent = path.parent().unwrap();
    let jobs = DirectorySource::new(BTreeMap::from([("job".into(), parent.to_path_buf())]))?;
    let job: ProfileDependencyJob = json(
        &jobs.read(
            "job",
            path.file_name()
                .unwrap()
                .to_str()
                .ok_or_else(|| invalid("Job filename"))?,
            JSON_LIMIT,
        )?,
    )?;
    if job.schema != "ddi-danoni-native-profile-dependency-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let source = DirectorySource::new(
        job.roots
            .into_iter()
            .map(|(k, p)| (k, if p.is_absolute() { p } else { parent.join(p) }))
            .collect(),
    )?;
    execute_profile(
        &job.request,
        &worker,
        &source,
        output,
        &job.limits,
        &mut |request, checkpoint| {
            checkpoint()?;
            job.responses.get(&request.url).cloned().ok_or_else(|| {
                Error::new(
                    "native-dependency-unavailable",
                    format!("No host-approved cached response for {}", request.url),
                )
            })
        },
        progress,
    )
}
