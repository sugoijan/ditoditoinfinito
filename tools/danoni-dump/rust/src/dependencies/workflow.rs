//! Adapt explicit plans and automatic profiles to the same bounded coordinator.
use super::*;
use crate::profiles::{self, ProfileRequest};
#[derive(Clone)]
pub(super) enum Workflow {
    Plan(Box<NativePlan>),
    Profile(Box<ProfileRequest>),
}
pub(super) struct Attempt {
    pub ready: bool,
    pub output: PathBuf,
    pub manifest: Value,
    pub failure: Option<Error>,
}
pub(super) struct CaptureProgress<'a> {
    pub stage: &'static str,
    pub capture_id: Option<&'a str>,
    pub captures: usize,
}
impl Workflow {
    pub fn request(&self) -> Result<Value> {
        match self {
            Self::Plan(p) => serde_json::to_value(p),
            Self::Profile(p) => serde_json::to_value(p),
        }
        .map_err(|e| invalid(e.to_string()))
    }
    pub fn schema(&self) -> &'static str {
        match self {
            Self::Plan(_) => "ddi-danoni-native-dependency-run/1",
            Self::Profile(_) => "ddi-danoni-native-profile-dependency-run/1",
        }
    }
    pub fn producer(&self) -> &'static str {
        match self {
            Self::Plan(_) => "native-dependency-closure/1",
            Self::Profile(_) => "native-profile-dependency-closure/1",
        }
    }
    pub fn seed(
        &self,
        source: &impl AssetSource,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<(Self, Memory)> {
        match self {
            Self::Plan(p) => {
                let prepared = orchestration::prepare_plan(p, source, checkpoint)?;
                let current: PlanJob = json(
                    &prepared
                        .files
                        .iter()
                        .find(|f| f.path == "replay-job.json")
                        .unwrap()
                        .bytes,
                )?;
                Ok((
                    Self::Plan(Box::new(current.plan)),
                    Memory(
                        prepared
                            .files
                            .into_iter()
                            .map(|f| (f.path, f.bytes))
                            .collect(),
                    ),
                ))
            }
            Self::Profile(p) => {
                let seed = profiles::prepare_seed(p, source, checkpoint)?;
                Ok((Self::Profile(Box::new(seed.request)), Memory(seed.files)))
            }
        }
    }
    /// Explicit plans reserve all known captures; profiles count at boundaries.
    pub fn budget(
        &self,
        source: &impl AssetSource,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<(u64, Option<usize>)> {
        match self {
            Self::Plan(p) => {
                let prepared = orchestration::prepare_plan(p, source, checkpoint)?;
                Ok((
                    prepared.files.iter().map(|f| f.bytes.len() as u64).sum(),
                    Some(prepared.captures.len()),
                ))
            }
            Self::Profile(p) => {
                let seed = profiles::prepare_seed(p, source, checkpoint)?;
                Ok((seed.files.values().map(|b| b.len() as u64).sum(), None))
            }
        }
    }

    pub fn capture_bytes(&self) -> u64 {
        match self {
            Self::Plan(p) => p.limits.output_bytes,
            Self::Profile(p) => p.capture_limits.output_bytes,
        }
    }
    pub fn run(
        &self,
        worker: &WorkerSpec,
        source: &impl AssetSource,
        output: &Path,
        remaining_captures: usize,
        remaining_ms: u64,
        progress: &mut impl FnMut(CaptureProgress<'_>) -> Result<()>,
    ) -> Result<Attempt> {
        match self {
            Self::Plan(p) => {
                let out = orchestration::execute(p, worker, source, output, &mut |p| {
                    progress(CaptureProgress {
                        stage: p.stage,
                        capture_id: p.capture_id.as_deref(),
                        captures: 0,
                    })
                })?;
                Ok(Attempt {
                    ready: out.ready,
                    output: out.output,
                    manifest: out.manifest,
                    failure: out.failure,
                })
            }
            Self::Profile(p) => {
                if remaining_captures == 0 || remaining_ms == 0 {
                    return Err(limited("No profile budget remains"));
                }
                let mut request = p.clone();
                request.limits.captures = request.limits.captures.min(remaining_captures);
                request.limits.wall_ms = request.limits.wall_ms.min(remaining_ms);
                let out = profiles::execute(&request, worker, source, output, &mut |p| {
                    progress(CaptureProgress {
                        stage: p.stage,
                        capture_id: None,
                        captures: p.captures,
                    })
                })?;
                Ok(Attempt {
                    ready: out.ready,
                    output: out.output,
                    manifest: out.manifest,
                    failure: out.failure,
                })
            }
        }
    }
    pub fn requests(
        &self,
        out: &Attempt,
        worker: &WorkerSpec,
        prefix: &str,
        memory: &Memory,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Vec<MissingDependency>> {
        match self {
            Self::Plan(_) => requests_from_attempt(out, worker, prefix, checkpoint),
            Self::Profile(p) => profile_requests(out, p, worker, prefix, memory, checkpoint),
        }
    }
    pub fn append(
        &mut self,
        request: &MissingDependency,
        value: &Accepted,
        memory: &mut Memory,
        attempt: usize,
    ) -> Result<()> {
        match self {
            Self::Plan(p) => {
                let ids = request
                    .witnesses
                    .iter()
                    .flat_map(|w| &w.chart_ids)
                    .collect::<BTreeSet<_>>();
                for chart in &mut p.charts {
                    if !ids.contains(&chart.id) {
                        continue;
                    }
                    let mut config: Value =
                        json(&memory.read("plan", &chart.config.path, 65536)?)?;
                    append_resource(&mut config, &value.resource, &request.url)?;
                    let b = bytes(&config)?;
                    supervisor::validate_config(&b)?;
                    let path = format!("inputs/dependency-{attempt:03}-{}.json", chart.id);
                    chart.config = bind(path.clone(), &b);
                    memory.0.insert(path, Arc::new(b));
                }
            }
            Self::Profile(p) => {
                let mut context: Value = json(&memory.read("profile", &p.context.path, 65536)?)?;
                append_resource(&mut context, &value.resource, &request.url)?;
                // Keep strict source-only context; the driver owns controls.
                let _: profiles::NativeContext =
                    serde_json::from_value(context.clone()).map_err(|e| invalid(e.to_string()))?;
                let b = bytes(&context)?;
                if b.len() > 65536 {
                    return Err(limited("Updated profile context exceeds 64 KiB"));
                }
                let path = format!("inputs/dependency-{attempt:03}-context.json");
                p.context = Binding {
                    namespace: "profile".into(),
                    path: path.clone(),
                    sha256: sha256(&b),
                };
                memory.0.insert(path, Arc::new(b));
            }
        }
        Ok(())
    }
}
fn append_resource(config: &mut Value, resource: &Value, url: &str) -> Result<()> {
    let rows = config["resources"]
        .as_array_mut()
        .ok_or_else(|| invalid("Missing resource table"))?;
    if rows.iter().any(|r| r["url"] == url) {
        return Err(Error::new(
            "native-dependency-no-progress",
            "Repeated dependency for the same source context",
        ));
    }
    rows.push(resource.clone());
    Ok(())
}

/// Classify the exact already-reviewed optional failure in a temporary audit
/// subset. Required failures remain untouched in the original capture; this
/// subset is never used to project a chart or establish successful execution.
fn pending_after_optional_review(
    capture: &Value,
    review: Option<&Value>,
) -> Result<Vec<RequestObservation>> {
    let mut pending = inspect_requests(capture)?;
    if let Some(review) = review
        && pending.iter().any(|r| r.original_url == "iro.txt")
    {
        let entries = capture["audit"]["entries"].as_array().unwrap();
        let message = "message=Asynchronous error occurred: Could not fetch: \"Invalid URL iro.txt: no exact GET match in immutable resource cache\" ";
        let mut subset = capture.clone();
        subset["audit"]["entries"] = json!(
            entries
                .iter()
                .filter(|e| match e["kind"].as_str() {
                    Some("fetch-denied") => e["detail"] == "iro.txt",
                    Some("runtime-error") => e["detail"] == message,
                    _ => true,
                })
                .cloned()
                .collect::<Vec<_>>()
        );
        subset["audit"]["runtimeErrors"] = json!(
            subset["audit"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "runtime-error")
                .count()
        );
        crate::extraction::review_optional_colors(&subset, review)?;
        pending.retain(|r| r.original_url != "iro.txt");
    }
    Ok(pending)
}
fn profile_requests(
    out: &Attempt,
    request: &ProfileRequest,
    worker: &WorkerSpec,
    prefix: &str,
    memory: &Memory,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<MissingDependency>> {
    if out.failure.as_ref().map(|e| e.code) != Some("native-profile-needs-dependencies") {
        return Err(out
            .failure
            .clone()
            .unwrap_or_else(|| invalid("Profile did not produce missing-request evidence")));
    }
    let source = DirectorySource::new(BTreeMap::from([("attempt".into(), out.output.clone())]))?;
    let optional = request
        .optional_color
        .as_ref()
        .map(|e| {
            let mut inputs = Inputs::new(memory);
            let review: Value = json(&inputs.checked(&e.review, JSON_LIMIT)?)?;
            let rows = review["missingResources"]
                .as_array()
                .ok_or_else(|| invalid("Missing optional-colour response table"))?;
            if rows.len() != e.missing_responses.len() {
                return Err(invalid("Optional-colour coverage differs"));
            }
            for row in rows {
                let matching = e
                    .missing_responses
                    .iter()
                    .filter(|b| row["path"] == b.path && row["sha256"] == b.sha256)
                    .collect::<Vec<_>>();
                if matching.len() != 1 {
                    return Err(invalid("Unbound optional-colour response"));
                }
                inputs.checked(matching[0], EVIDENCE_LIMIT)?;
            }
            Ok(review)
        })
        .transpose()?;
    let pending = out.manifest["pendingDependencies"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 64)
        .ok_or_else(|| invalid("Missing bounded pending profile probes"))?;
    let mut grouped: BTreeMap<String, MissingDependency> = BTreeMap::new();
    for pending in pending {
        checkpoint()?;
        let bindings: crate::extraction::CaptureBindings =
            serde_json::from_value(pending["capture"].clone())
                .map_err(|e| invalid(e.to_string()))?;
        // The immutable pending snapshot must belong to one successful child.
        let captures = out.manifest["captures"]
            .as_object()
            .ok_or_else(|| invalid("Missing profile child receipts"))?;
        let matching = captures
            .iter()
            .filter(|(id, _)| {
                bindings.snapshot.path == format!("probes/{id}/capture/snapshot.json")
            })
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(invalid("Unmapped profile request capture"));
        }
        let (id, row) = matching[0];
        relative_path(id)?;
        let child_prefix = format!("probes/{id}");
        let receipt_path = format!("{child_prefix}/manifest.json");
        let child_bytes = file(&source, &receipt_path, JSON_LIMIT)?;
        if row["ready"] != true
            || row["receipt"]["path"] != receipt_path
            || row["receipt"]["sha256"] != sha256(&child_bytes)
        {
            return Err(invalid("Profile child receipt differs"));
        }
        let child: Value = json(&child_bytes)?;
        if child["ready"] != true
            || child["build"] != supervisor::expected_build_identity()
            || child["worker"]
                != serde_json::to_value(worker).map_err(|e| invalid(e.to_string()))?
        {
            return Err(invalid("Profile child worker differs"));
        }
        let snapshot_bytes = file(&source, &bindings.snapshot.path, 32 * 1024 * 1024)?;
        let capture: Value = json(&snapshot_bytes)?;
        let run_bytes = file(&source, &format!("{child_prefix}/run.json"), JSON_LIMIT)?;
        let run: Value = json(&run_bytes)?;
        let config_bytes = file(
            &source,
            &format!("{child_prefix}/inputs/capture.json"),
            65536,
        )?;
        let config: Value = json(&config_bytes)?;
        let original_config = file(&source, &bindings.config.path, 65536)?;
        if bindings.snapshot.namespace != "profile"
            || bindings.run.namespace != "profile"
            || bindings.config.namespace != "profile"
            || bindings.run.path != format!("{child_prefix}/run.json")
            || sha256(&snapshot_bytes) != bindings.snapshot.sha256
            || sha256(&snapshot_bytes) != child["bindings"]["capture"]["snapshot"]["sha256"]
            || sha256(&run_bytes) != bindings.run.sha256
            || sha256(&run_bytes) != child["bindings"]["capture"]["run"]["sha256"]
            || original_config != config_bytes
            || sha256(&config_bytes) != bindings.config.sha256
            || sha256(&config_bytes) != run["configSha256"]
            || run != child["run"]
            || run["exitCode"] != 0
            || !run["termination"].is_null()
            || run["inputsUnchanged"] != true
            || run["workerSha256"] != worker.sha256
            || capture["source"]["config"] != config
            || capture["source"]["movieSha256"] != request.movie.sha256
        {
            return Err(invalid("Profile request/config/run/movie binding differs"));
        }
        let selector = pending["selector"].as_u64().filter(|n| *n < 16);
        if !pending["selector"].is_null() && selector.is_none() {
            return Err(invalid("Invalid pending profile selector"));
        }
        for observation in pending_after_optional_review(&capture, optional.as_ref())? {
            grouped
                .entry(observation.url.clone())
                .or_insert_with(|| MissingDependency {
                    url: observation.url,
                    maximum_bytes: 16 * 1024 * 1024,
                    witnesses: vec![],
                })
                .witnesses
                .push(RequestWitness {
                    capture_id: id.clone(),
                    chart_ids: selector
                        .map(|n| vec![format!("chart-{n}")])
                        .unwrap_or_default(),
                    iteration: observation.iteration,
                    original_url: observation.original_url,
                    snapshot: Binding {
                        namespace: "dependency".into(),
                        path: format!("{prefix}/{}", bindings.snapshot.path),
                        sha256: bindings.snapshot.sha256.clone(),
                    },
                    source_url: config["sourceUrl"].as_str().unwrap().into(),
                    page_url: config["pageUrl"].as_str().map(str::to_string),
                });
        }
    }
    Ok(grouped.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Value, Value) {
        let errors=["dos.txt","iro.txt"].map(|name|json!({"kind":"runtime-error","iteration":2,"truncated":false,"detail":format!("message=Asynchronous error occurred: Could not fetch: \"Invalid URL {name}: no exact GET match in immutable resource cache\" ")}));
        let mut entries = vec![];
        for name in ["dos.txt", "iro.txt"] {
            entries.push(json!({"kind":"fetch-denied","iteration":2,"truncated":false,"detail":name,"request":{"schema":"ddi-native-request/1","method":"GET","hasBody":false,"hasHeaders":false,"originalUrl":name,"resolvedUrl":format!("https://example.test/work/{name}"),"reason":"cache-miss"}}));
        }
        entries.extend(errors);
        let capture = json!({"schema":"ddi-ruffle-probe/experimental-1","ruffleRevision":supervisor::REVISION,"build":supervisor::expected_build_identity(),"avm1Halted":false,"source":{"movieSha256":"a".repeat(64),"config":{"movie":"movie.swf","sourceUrl":"https://example.test/work/song.swf","captureFrames":[45,46],"resources":[]}},"audit":{"entries":entries,"runtimeErrors":2,"truncated":false,"droppedEntries":0,"truncatedDetails":0}});
        let review = json!({"schema":"ddi-native-optional-color-review/1","sourceSwfSha256":"a".repeat(64),"missingResources":[{"url":"https://example.test/work/iro.txt","status":404,"retrievedAt":"2026-10-14T00:00:00Z","sha256":"b".repeat(64),"path":"response.html"}]});
        (capture, review)
    }
    #[test]
    fn optional_404_is_not_refetched_and_required_failure_is_not_suppressed() {
        let (capture, review) = fixture();
        let original = capture.clone();
        assert_eq!(
            pending_after_optional_review(&capture, None).unwrap().len(),
            2
        );
        let requests = pending_after_optional_review(&capture, Some(&review)).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].original_url, "dos.txt");
        assert_eq!(capture, original); // Classification is never a normalized capture.
        assert!(crate::extraction::review_optional_colors(&capture, &review).is_err());
    }
    #[test]
    fn optional_request_classification_requires_exact_source_and_diagnostics() {
        let (capture, review) = fixture();
        for bad in [
            "movie",
            "status",
            "url",
            "error",
            "iteration",
            "duplicate",
            "method",
            "truncated",
        ] {
            let mut c = capture.clone();
            let mut r = review.clone();
            match bad {
                "movie" => r["sourceSwfSha256"] = json!("c".repeat(64)),
                "status" => r["missingResources"][0]["status"] = json!(200),
                "url" => r["missingResources"][0]["url"] = json!("https://example.test/iro.txt"),
                "error" => c["audit"]["entries"][3]["detail"] = json!("different error"),
                "iteration" => c["audit"]["entries"][3]["iteration"] = json!(3),
                "duplicate" => {
                    let v = c["audit"]["entries"][1].clone();
                    c["audit"]["entries"].as_array_mut().unwrap().push(v);
                }
                "method" => c["audit"]["entries"][0]["request"]["method"] = json!("POST"),
                "truncated" => c["audit"]["truncated"] = json!(true),
                _ => unreachable!(),
            }
            assert!(
                pending_after_optional_review(&c, Some(&r)).is_err(),
                "{bad}"
            );
        }
    }
}
