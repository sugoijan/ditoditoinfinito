use super::*;
use crate::{
    assets::*,
    dependencies,
    extraction::*,
    model::Metadata,
    native::{self, DirectorySource},
    orchestration::{self, ChartPlan, NativePlan, Observation, PlanJob},
    supervisor::{self, CaptureRequest, Limits, WorkerSpec},
    swf,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextResource {
    pub url: String,
    pub path: String,
    pub sha256: String,
    pub http_charset: Option<String>,
}
/// Original script context only; selectors and observations are profile-owned.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeContext {
    pub movie: String,
    pub source_url: String,
    pub page_url: Option<String>,
    #[serde(default)]
    pub flash_vars: BTreeMap<String, String>,
    #[serde(default)]
    pub resources: Vec<ContextResource>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ProfileLimits {
    pub captures: usize,
    pub motion_trials: usize,
    pub wall_ms: u64,
    pub output_bytes: u64,
}
impl Default for ProfileLimits {
    fn default() -> Self {
        Self {
            captures: 64,
            motion_trials: 8,
            wall_ms: 180000,
            output_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}
impl ProfileLimits {
    pub(crate) fn validate(&self) -> Result<()> {
        if !(1..=64).contains(&self.captures)
            || !(1..=12).contains(&self.motion_trials)
            || !(1..=300000).contains(&self.wall_ms)
            || !(1024..=2 * 1024 * 1024 * 1024).contains(&self.output_bytes)
        {
            return Err(Error::new(
                "native-profile-limit",
                "Profile budgets exceed reviewed maxima",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileRequest {
    /// Optional user-selected menu rows; omission means every supported row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selectors: Option<Vec<usize>>,
    pub movie: Binding,
    pub source_manifest: Binding,
    pub context: Binding,
    pub optional_color: Option<OptionalColorEvidence>,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub limits: ProfileLimits,
    #[serde(default)]
    pub capture_limits: Limits,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileProgress {
    pub stage: &'static str,
    pub captures: usize,
    pub selector: Option<usize>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOutcome {
    pub ready: bool,
    pub output: PathBuf,
    pub manifest: Value,
    pub failure: Option<Error>,
}
fn bytes(v: &impl Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(v).map_err(|e| invalid(e.to_string()))
}
fn bind(path: impl Into<String>, b: &[u8]) -> Binding {
    Binding {
        namespace: "profile".into(),
        path: path.into(),
        sha256: sha256(b),
    }
}
struct Prepared {
    files: BTreeMap<String, Arc<Vec<u8>>>,
    context: Value,
    movie: Binding,
    source: Binding,
    profile: EngineProfile,
    fingerprints: Vec<swf::ActionFingerprint>,
    source_inputs: BTreeMap<String, Binding>,
    optional: Option<OptionalColorEvidence>,
}
fn add(
    files: &mut BTreeMap<String, Arc<Vec<u8>>>,
    path: String,
    b: Arc<Vec<u8>>,
) -> Result<Binding> {
    relative_path(&path)?;
    if let Some(previous) = files.get(&path) {
        if **previous != *b {
            return Err(invalid("Conflicting profile input"));
        }
    } else {
        if files.len() >= 4096
            || files
                .values()
                .map(|b| b.len())
                .sum::<usize>()
                .checked_add(b.len())
                .is_none_or(|n| n > 128 * 1024 * 1024)
        {
            return Err(invalid("Profile input limit"));
        }
        files.insert(path.clone(), b.clone());
    }
    Ok(bind(path, &b))
}
fn blob(files: &mut BTreeMap<String, Arc<Vec<u8>>>, b: Arc<Vec<u8>>) -> Result<Binding> {
    add(files, format!("inputs/blobs/{}", sha256(&b)), b)
}
fn config(
    context: &Value,
    events: &[Value],
    frames: &[u64],
    mix: bool,
    render: bool,
    execution_ms: u64,
) -> Result<Vec<u8>> {
    let mut value = context.clone();
    value["events"] = json!(events);
    value["captureFrames"] = json!(frames);
    value["render"] = json!(render);
    value["mixAudio"] = json!(mix);
    value["inspectionScale"] = json!(4);
    value["avmExecutionMs"] = json!(execution_ms);
    let b = bytes(&value)?;
    supervisor::validate_config(&b)?;
    Ok(b)
}
fn prepare(
    request: &ProfileRequest,
    source: &impl AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Prepared> {
    request.limits.validate()?;
    request.capture_limits.validate()?;
    if let Some(indices) = &request.selectors
        && (indices.is_empty()
            || indices.len() > 16
            || indices.iter().any(|i| *i >= 16)
            || indices.windows(2).any(|p| p[0] >= p[1]))
    {
        return Err(invalid(
            "Need 1..16 unique increasing selector indices in 0..16",
        ));
    }
    let mut inputs = Inputs::new(source);
    let movie_bytes = inputs.checked(&request.movie, swf::MOVIE_LIMIT)?;
    let movie = swf::Movie::read(&movie_bytes, checkpoint)?;
    let (profile, fingerprints) = identity::inspect(&movie, checkpoint)?;
    let raw_context = inputs.checked(&request.context, 65536)?;
    let context: NativeContext = json(&raw_context)?;
    let configured = request.context.sibling(&RelativeBinding {
        path: context.movie.clone(),
        sha256: request.movie.sha256.clone(),
    })?;
    inputs.checked(&configured, swf::MOVIE_LIMIT)?;
    let provenance = inputs.checked(&request.source_manifest, JSON_LIMIT)?;
    let p: Value = json(&provenance)?;
    if p.get("sourceSwfSha256")
        .unwrap_or(&p["swf"]["sha256"])
        .as_str()
        != Some(request.movie.sha256.as_str())
    {
        return Err(invalid("Source manifest names another movie"));
    }
    let mut value = serde_json::to_value(&context).map_err(|e| invalid(e.to_string()))?;
    config(
        &value,
        &[],
        &[1],
        false,
        profile.render(),
        profile.avm_execution_ms(),
    )?;
    let mut files = BTreeMap::new();
    let movie = blob(&mut files, movie_bytes)?;
    let source = add(&mut files, "inputs/source-manifest.json".into(), provenance)?;
    add(
        &mut files,
        "inputs/original-context.json".into(),
        raw_context,
    )?;
    value["movie"] = json!(format!("blobs/{}", movie.sha256));
    let mut total = 0;
    for row in value["resources"].as_array_mut().unwrap() {
        checkpoint()?;
        let rel: RelativeBinding =
            serde_json::from_value(json!({"path":row["path"],"sha256":row["sha256"]}))
                .map_err(|e| invalid(e.to_string()))?;
        let b = inputs.checked(&request.context.sibling(&rel)?, 16 * 1024 * 1024)?;
        total += b.len();
        if total > 64 * 1024 * 1024 {
            return Err(invalid("Profile resource limit"));
        }
        let dest = blob(&mut files, b)?;
        row["path"] = json!(format!("blobs/{}", dest.sha256));
    }
    let optional = request
        .optional_color
        .as_ref()
        .map(|e| {
            let mut evidence = orchestration::optional(e, "source", &mut inputs, &mut files)?;
            evidence.review.namespace = "profile".into();
            for response in &mut evidence.missing_responses {
                response.namespace = "profile".into();
            }
            Ok(evidence)
        })
        .transpose()?;
    Ok(Prepared {
        files,
        context: value,
        movie,
        source,
        profile,
        fingerprints,
        source_inputs: inputs.used,
        optional,
    })
}
/// Immutable source-only replay seed used by dependency orchestration.
pub(crate) struct ProfileSeed {
    pub request: ProfileRequest,
    pub files: BTreeMap<String, Arc<Vec<u8>>>,
}
pub(crate) fn prepare_seed(
    request: &ProfileRequest,
    source: &impl AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<ProfileSeed> {
    let prepared = prepare(request, source, checkpoint)?;
    let mut files = prepared.files;
    let context = add(
        &mut files,
        "inputs/context.json".into(),
        Arc::new(bytes(&prepared.context)?),
    )?;
    let request = ProfileRequest {
        selectors: request.selectors.clone(),
        movie: prepared.movie,
        source_manifest: prepared.source,
        context,
        optional_color: prepared.optional,
        metadata: request.metadata.clone(),
        limits: request.limits.clone(),
        capture_limits: request.capture_limits.clone(),
    };
    let job = ProfileJob {
        schema: "ddi-danoni-native-profile-job/1".into(),
        roots: BTreeMap::from([("profile".into(), PathBuf::from("."))]),
        request: request.clone(),
    };
    add(
        &mut files,
        "profile-job.json".into(),
        Arc::new(bytes(&job)?),
    )?;
    Ok(ProfileSeed { request, files })
}
struct Run<'a, F: FnMut(&ProfileProgress) -> Result<()>> {
    root: &'a Path,
    worker: &'a WorkerSpec,
    request: &'a ProfileRequest,
    prepared: &'a Prepared,
    progress: &'a mut F,
    started: Instant,
    last_size: Instant,
    state: ProfileProgress,
    captures: BTreeMap<String, Value>,
    pending: Vec<Value>,
    trials: Vec<Value>,
}
impl<F: FnMut(&ProfileProgress) -> Result<()>> Run<'_, F> {
    fn reserve(&self, extra: u64) -> Result<()> {
        if orchestration::tree_size(self.root)?
            .checked_add(extra)
            .is_none_or(|n| n > self.request.limits.output_bytes)
        {
            return Err(Error::new(
                "native-profile-limit",
                "Profile output reservation exceeded",
            ));
        }
        Ok(())
    }
    fn check(&mut self) -> Result<()> {
        if self.started.elapsed().as_millis() > self.request.limits.wall_ms as u128 {
            return Err(Error::new(
                "native-profile-limit",
                "Profile wall budget exhausted",
            ));
        }
        if self.last_size.elapsed() >= Duration::from_millis(50) || self.state.stage == "ready" {
            self.reserve(0)?;
            self.last_size = Instant::now();
        }
        (self.progress)(&self.state)?;
        if self.started.elapsed().as_millis() > self.request.limits.wall_ms as u128 {
            return Err(Error::new(
                "native-profile-limit",
                "Profile wall budget exhausted in host callback",
            ));
        }
        Ok(())
    }
    fn source(&self) -> Result<DirectorySource> {
        DirectorySource::new(BTreeMap::from([(
            "profile".into(),
            self.root.to_path_buf(),
        )]))
    }
    fn pcm_binding(&self, binding: &CaptureBindings) -> Result<Binding> {
        let path = binding.snapshot.path.replace("snapshot.json", "audio.pcm");
        Ok(Binding {
            namespace: "profile".into(),
            sha256: sha256(&self.source()?.read("profile", &path, MEDIA_LIMIT)?),
            path,
        })
    }
    fn capture(
        &mut self,
        id: &str,
        events: &[Value],
        frames: &[u64],
        mix: bool,
    ) -> Result<(Value, CaptureBindings)> {
        self.check()?;
        if self.state.captures >= self.request.limits.captures {
            return Err(Error::new(
                "native-profile-limit",
                "Profile capture budget exhausted",
            ));
        }
        self.reserve(
            self.prepared
                .files
                .values()
                .map(|b| b.len() as u64)
                .sum::<u64>()
                + orchestration::worker_bytes(self.worker)?
                + self.request.capture_limits.output_bytes
                + 578 * 1024 * 1024,
        )?;
        let b = config(
            &self.prepared.context,
            events,
            frames,
            mix,
            self.prepared.profile.render(),
            self.prepared.profile.avm_execution_ms(),
        )?;
        let binding = bind(format!("inputs/{id}.json"), &b);
        orchestration::write(self.root, &binding.path, &b)?;
        let request = CaptureRequest {
            config: binding.clone(),
            movie: self.prepared.movie.clone(),
            provenance: Some(self.prepared.source.clone()),
            limits: self.request.capture_limits.clone(),
        };
        let source = self.source()?;
        self.state.captures += 1;
        let out = supervisor::capture(
            &request,
            self.worker,
            &source,
            &self.root.join("probes").join(id),
            &mut || self.check(),
        )?;
        let receipt = format!(
            "probes/{id}/{}",
            if out.ready {
                "manifest.json"
            } else {
                "failure.json"
            }
        );
        let raw = source.read("profile", &receipt, JSON_LIMIT)?;
        self.captures.insert(id.into(),json!({"ready":out.ready,"receipt":{"path":receipt,"sha256":sha256(&raw)},"failure":out.failure}));
        if let Some(e) = out.failure {
            return Err(e);
        }
        let manifest = out
            .manifest
            .ok_or_else(|| invalid("Missing successful capture receipt"))?;
        let rebound = |key: &str| -> Result<Binding> {
            let b: Binding = serde_json::from_value(manifest["bindings"]["capture"][key].clone())
                .map_err(|e| invalid(e.to_string()))?;
            Ok(Binding {
                namespace: "profile".into(),
                path: format!("probes/{id}/{}", b.path),
                sha256: b.sha256,
            })
        };
        let bindings = CaptureBindings {
            snapshot: rebound("snapshot")?,
            run: rebound("run")?,
            config: binding,
        };
        let snapshot = source.read("profile", &bindings.snapshot.path, 32 * 1024 * 1024)?;
        if sha256(&snapshot) != bindings.snapshot.sha256 {
            return Err(invalid("Profile snapshot hash changed"));
        }
        let mut capture: Value = json(&snapshot)?;
        // Discover unrelated required GETs before applying the exact optional
        // colour exception: that exception deliberately rejects extra failures.
        // No audit entry is suppressed while dependencies are still missing.
        let mut missing = dependencies::inspect_requests(&capture)?;
        let review_deferred =
            self.prepared.optional.is_some() && missing.iter().any(|r| r.original_url != "iro.txt");
        if let Some(e) = &self.prepared.optional
            && !review_deferred
        {
            let raw = source.read("profile", &e.review.path, JSON_LIMIT)?;
            if sha256(&raw) != e.review.sha256 {
                return Err(invalid("Optional-colour review hash differs"));
            }
            let review: Value = json(&raw)?;
            capture = review_optional_colors(&capture, &review)?.0;
            missing = dependencies::inspect_requests(&capture)?;
        }
        if !missing.is_empty() {
            self.pending
                .push(json!({"capture":bindings,"requests":missing,"optionalColorReviewDeferred":review_deferred,"selector":self.state.selector}));
            return Err(Error::new(
                "native-profile-needs-dependencies",
                "Observed missing GETs must be resolved in the source context before menu planning",
            ));
        }
        self.prepared.profile.check_render_support(&capture)?;
        Ok((capture, bindings))
    }
}
/// Observe a reviewed native menu, drive every supported selector, prove one
/// generic motion witness per chart, and run unchanged full normalization.
/// The context must already contain required public cached resources. Missing
/// requests are retained as evidence, never fetched or fabricated here.
pub fn execute(
    request: &ProfileRequest,
    worker: &WorkerSpec,
    source: &impl AssetSource,
    output: &Path,
    progress: &mut impl FnMut(&ProfileProgress) -> Result<()>,
) -> Result<ProfileOutcome> {
    let started = Instant::now();
    let state = ProfileProgress {
        stage: "planning",
        captures: 0,
        selector: None,
    };
    let prepared = prepare(request, source, &mut || {
        progress(&state)?;
        if started.elapsed().as_millis() > request.limits.wall_ms as u128 {
            return Err(Error::new(
                "native-profile-limit",
                "Profile planning wall budget exhausted",
            ));
        }
        Ok(())
    })?;
    let mut owned = orchestration::new_root(output)?;
    for (path, b) in &prepared.files {
        progress(&state)?;
        if orchestration::tree_size(&owned.root)?
            .checked_add(b.len() as u64)
            .is_none_or(|n| n > request.limits.output_bytes)
        {
            return Err(Error::new(
                "native-profile-limit",
                "Profile staging budget exhausted",
            ));
        }
        orchestration::write(&owned.root, path, b)?;
    }
    fs::create_dir(owned.root.join("probes")).map_err(native::io)?;
    let mut run = Run {
        root: &owned.root,
        worker,
        request,
        prepared: &prepared,
        progress,
        started,
        last_size: Instant::now(),
        state,
        captures: BTreeMap::new(),
        pending: vec![],
        trials: vec![],
    };
    let mut observed_menu = None;
    let mut excluded_selectors = Vec::new();
    let mut planned = Vec::new();
    let mut play_inputs = Vec::new();
    let result = (|| -> Result<Value> {
        run.state.stage = "menu";
        let (capture, menu_binding) = run.capture(
            "menu",
            &[json!({"frame":40,"type":"click","x":160,"y":prepared.profile.title_y()})],
            &[45, 46],
            false,
        )?;
        let menu = prepared.profile.read_menu(&capture)?;
        observed_menu = Some(json!({"menu":menu,"capture":menu_binding}));
        if let Some(indices) = &request.selectors
            && indices.iter().any(|i| *i >= menu.rows.len())
        {
            return Err(invalid("Requested selector is outside the observed menu"));
        }
        let mut charts = Vec::new();
        for row in &menu.rows {
            if request
                .selectors
                .as_ref()
                .is_some_and(|indices| !indices.contains(&row.index))
            {
                excluded_selectors.push(json!({"row":row,"reason":"unselected-selector"}));
                continue;
            }
            let Some(mode) = row.mode else {
                excluded_selectors.push(json!({"row":row,"reason":"unsupported-key-label"}));
                continue;
            };
            run.state.selector = Some(row.index);
            run.state.stage = "initialization";
            let controls = prepared.profile.selector_controls(&menu, row.index)?;
            let mut events = controls.events.clone();
            let opts = prepared.profile.options(mode)?;
            let mut frames = controls.initialization_frames.clone();
            frames.push(prepared.profile.discovery_end());
            frames.sort_unstable();
            frames.dedup();
            let (mut capture, mut projection_binding) = run.capture(
                &format!("chart-{}-init", row.index),
                &controls.events,
                &frames,
                true,
            )?;
            let confirm_selector = |capture: &Value| -> Result<()> {
                let mut selected = capture.clone();
                selected["captures"] = json!(
                    capture["captures"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|c| c["frame"]
                            .as_u64()
                            .is_some_and(|f| f == controls.start - 2 || f == controls.start - 1))
                        .cloned()
                        .collect::<Vec<_>>()
                );
                prepared.profile.confirm_start_settings(&selected)?;
                let selected = prepared.profile.read_menu(&selected)?;
                if selected.rows != menu.rows || selected.selected != row.index {
                    return Err(invalid("Observed selected menu differs from planned row"));
                }
                Ok(())
            };
            confirm_selector(&capture)?;

            let starts = capture["audio"]["streamStarts"]
                .as_array()
                .ok_or_else(|| invalid("Missing native stream starts"))?
                .clone();
            let streams = starts
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s["compression"] == "Mp3"
                        && s["iteration"].as_u64().is_some_and(|f| {
                            f >= controls.start && f <= prepared.profile.discovery_end()
                        })
                })
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if streams.len() != 1 {
                return Err(invalid(
                    "Need exactly one observed chart MP3 start within the profile discovery interval",
                ));
            }
            let stream = streams[0];
            let start_frame = starts[stream]["iteration"].as_u64().unwrap();
            if !frames.contains(&start_frame) {
                frames.push(start_frame);
                frames.sort_unstable();
                (capture, projection_binding) = run.capture(
                    &format!("chart-{}-audio", row.index),
                    &controls.events,
                    &frames,
                    true,
                )?;
                if capture["audio"]["streamStarts"] != json!(starts) {
                    return Err(invalid("Stream starts changed during exact-origin replay"));
                }
            }
            confirm_selector(&capture)?;
            let projection = project_capture(&capture, &opts, &mut || run.check())?;
            let clock = projection.chart["clockField"].as_str();
            let origin = movie_audio_origin(&capture, clock, stream)?;
            let candidates = prepared.profile.motion_candidates(
                &capture,
                &projection.chart,
                &origin,
                &mut || run.check(),
            )?;
            let mut proved = None;
            for (index, candidate) in candidates
                .into_iter()
                .take(request.limits.motion_trials)
                .enumerate()
            {
                run.state.stage = "motion";
                let (motion_capture, binding) = run.capture(
                    &format!("chart-{}-motion-{index}", row.index),
                    &controls.events,
                    &candidate.iterations,
                    true,
                )?;
                let result = movie_observation(
                    &motion_capture,
                    &projection.chart,
                    &candidate,
                    clock,
                    &mut || run.check(),
                )
                .and_then(|m| {
                    validate_clock_agreement(&origin, &m)?;
                    Ok(m)
                });
                run.trials.push(json!({"selector":row.index,"candidate":candidate,"capture":binding,"passed":result.is_ok(),"failure":result.as_ref().err()}));
                if let Ok(measured) = result {
                    proved = Some((candidate, binding, measured));
                    break;
                }
            }
            let (selection, mut motion_binding, mut measured) = proved.ok_or_else(|| {
                Error::new(
                    "native-profile-motion-unproved",
                    "No bounded automatic motion proposal passed the unchanged observation proof",
                )
            })?;
            let id = format!("chart-{}", row.index);
            let mut native_pcm = run.pcm_binding(&projection_binding)?;
            if has_early_holds(&projection.chart)? {
                // Reuse the complete normalizer as the probe. Only its exact
                // insufficient-audio result can trigger physical support inputs.
                run.state.stage = "audio-check";
                run.reserve(576 * 1024 * 1024)?;
                let probe_request = NativeWorkRequest {
                    source_manifest: prepared.source.clone(),
                    movie: prepared.movie.clone(),
                    metadata: request.metadata.clone(),
                    charts: vec![NativeChartRequest {
                        id: id.clone(),
                        label: Some(row.label.clone()),
                        options: opts.clone(),
                        projection: projection_binding.clone(),
                        motion: motion_binding.clone(),
                        motion_selection: selection.clone(),
                        audio: projection_binding.clone(),
                        native_pcm: native_pcm.clone(),
                        stream_start_index: stream,
                        optional_color: prepared.optional.clone(),
                    }],
                };
                match prepare_native_work(&probe_request, &run.source()?, &mut || run.check()) {
                    Ok(_) => {}
                    Err(e) if e.code == "insufficient-audio-origin" => {
                        let seed_projection = projection_binding.clone();
                        let seed_motion = motion_binding.clone();
                        let derived = early_hold_inputs(
                            &projection.chart,
                            &origin,
                            *controls.initialization_frames.last().unwrap(),
                            64 - events.len(),
                            &mut || run.check(),
                        )?;
                        if derived.is_empty() {
                            return Err(e);
                        }
                        events.extend(derived.clone());
                        let trial = play_inputs.len();
                        play_inputs.push(json!({"selector":row.index,"events":derived,"seedProjection":seed_projection,"seedMotion":seed_motion,"seedNormalizationFailure":e,"projectionUnchanged":null,"audioOriginUnchanged":null,"motionProved":false,"scope":"Physical hold-intro inputs shared by every accepted role; no source, clock or gauge patch. Final full normalization is separate."}));
                        run.state.stage = "play-inputs";
                        let (replayed, binding) = run.capture(
                            &format!("chart-{}-held-audio", row.index),
                            &events,
                            &frames,
                            true,
                        )?;
                        confirm_selector(&replayed)?;
                        let replayed_projection =
                            project_capture(&replayed, &opts, &mut || run.check())?;
                        if replayed_projection.chart != projection.chart
                            || movie_audio_origin(&replayed, clock, stream)? != origin
                        {
                            return Err(Error::new(
                                "native-profile-play-inputs-changed-source",
                                "Derived play inputs changed chart data or the native audio origin",
                            ));
                        }
                        projection_binding = binding;
                        native_pcm = run.pcm_binding(&projection_binding)?;
                        let (motion_capture, binding) = run.capture(
                            &format!("chart-{}-held-motion", row.index),
                            &events,
                            &selection.iterations,
                            true,
                        )?;
                        measured = movie_observation(
                            &motion_capture,
                            &projection.chart,
                            &selection,
                            clock,
                            &mut || run.check(),
                        )?;
                        validate_clock_agreement(&origin, &measured)?;
                        motion_binding = binding;
                        play_inputs[trial]["projection"] = json!(projection_binding);
                        play_inputs[trial]["motion"] = json!(motion_binding);
                        play_inputs[trial]["projectionUnchanged"] = json!(true);
                        play_inputs[trial]["audioOriginUnchanged"] = json!(true);
                        play_inputs[trial]["motionProved"] = json!(true);
                    }
                    Err(e) => return Err(e),
                }
            }
            charts.push(NativeChartRequest {
                id: id.clone(),
                label: Some(row.label.clone()),
                options: opts.clone(),
                projection: projection_binding.clone(),
                motion: motion_binding.clone(),
                motion_selection: selection.clone(),
                audio: projection_binding.clone(),
                native_pcm,
                stream_start_index: stream,
                optional_color: prepared.optional.clone(),
            });
            planned.push(ChartPlan {
                id,
                label: Some(row.label.clone()),
                options: opts,
                config: projection_binding.config,
                projection: Observation {
                    capture_frames: frames.clone(),
                    render: prepared.profile.render(),
                    mix_audio: true,
                    avm_execution_ms: None,
                },
                motion: Observation {
                    capture_frames: selection.iterations.clone(),
                    render: prepared.profile.render(),
                    mix_audio: true,
                    avm_execution_ms: None,
                },
                motion_selection: selection,
                audio: Observation {
                    capture_frames: frames,
                    render: prepared.profile.render(),
                    mix_audio: true,
                    avm_execution_ms: None,
                },
                stream_start_index: stream,
                optional_color: prepared.optional.clone(),
            });
            orchestration::write(
                run.root,
                &format!("motion-{}.json", row.index),
                &bytes(&measured)?,
            )?;
        }
        if charts.is_empty() {
            return Err(Error::new(
                "unsupported-native-profile-modes",
                "Observed menu has no supported 5/7 selectors",
            ));
        }
        let normal_request = NativeWorkRequest {
            source_manifest: prepared.source.clone(),
            movie: prepared.movie.clone(),
            metadata: request.metadata.clone(),
            charts,
        };
        orchestration::write(
            run.root,
            "normalization-request.json",
            &bytes(&normal_request)?,
        )?;
        let plan = NativePlan {
            source_manifest: prepared.source.clone(),
            movie: prepared.movie.clone(),
            metadata: request.metadata.clone(),
            charts: planned.clone(),
            limits: request.capture_limits.clone(),
        };
        let job = PlanJob {
            schema: "ddi-danoni-native-plan-job/1".into(),
            roots: BTreeMap::from([("profile".into(), PathBuf::from("."))]),
            plan,
        };
        orchestration::write(run.root, "replay-job.json", &bytes(&job)?)?;
        run.state.stage = "normalization";
        run.reserve(576 * 1024 * 1024)?;
        let work = prepare_native_work(&normal_request, &run.source()?, &mut || run.check())?;
        native::write_normalized(&work, &run.root.join("normalized"), &mut || run.check())?;
        Ok(
            json!({"manifest":"normalized/manifest.json","manifestSha256":sha256(&run.source()?.read("profile","normalized/manifest.json",JSON_LIMIT)?),"normalization":"normalized/normalization.json","normalizationSha256":sha256(&run.source()?.read("profile","normalized/normalization.json",JSON_LIMIT)?),"charts":work.manifest["charts"].as_array().unwrap().len()}),
        )
    })();
    let (normalized, mut failure) = match result {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    let mut manifest = json!({"schema":"ddi-danoni-native-profile-run/1","producer":"native-menu-profiles/1","ready":failure.is_none(),"profile":prepared.profile,"request":request,"worker":worker,"fingerprints":prepared.fingerprints,"fingerprintScope":"Reviewed control/placement roles plus every AVM2 tag; complete bounded tag scan, not a full script audit","sourceInputs":prepared.source_inputs,"menu":observed_menu,"excludedSelectors":excluded_selectors,"failedSelector":if failure.is_some(){run.state.selector}else{None},"captures":run.captures,"motionTrials":run.trials,"playInputs":play_inputs,"pendingDependencies":run.pending,"plannedCharts":planned,"normalized":normalized,"failure":failure,"scope":"Requested observed supported menu selectors and proved source/motion/PCM normalization; no HTTP, hidden-chart completeness, tempo acceptance or installation"});
    if failure.is_none() {
        orchestration::write(run.root, ".manifest.json.tmp", &bytes(&manifest)?)?;
        run.state.stage = "ready";
        if let Err(e) = run.check() {
            fs::remove_file(run.root.join(".manifest.json.tmp")).map_err(native::io)?;
            failure = Some(e);
            manifest["ready"] = json!(false);
            manifest["normalized"] = Value::Null;
            manifest["failure"] = json!(failure);
        } else {
            fs::rename(
                run.root.join(".manifest.json.tmp"),
                run.root.join("manifest.json"),
            )
            .map_err(native::io)?;
        }
    }
    if failure.is_some() {
        orchestration::atomic(run.root, "failure.json", &manifest)?;
    }
    let output = run.root.to_path_buf();
    owned.retain = true;
    Ok(ProfileOutcome {
        ready: failure.is_none(),
        output,
        manifest,
        failure,
    })
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub request: ProfileRequest,
}
pub fn execute_job(
    bundle: &Path,
    job: &Path,
    output: &Path,
    progress: &mut impl FnMut(&ProfileProgress) -> Result<()>,
) -> Result<ProfileOutcome> {
    let worker = crate::worker_build::load_bundle(bundle, &mut || {
        progress(&ProfileProgress {
            stage: "worker",
            captures: 0,
            selector: None,
        })
    })?;
    let path = job.canonicalize().map_err(native::io)?;
    native::external(&path)?;
    let parent = path.parent().unwrap();
    let jobs = DirectorySource::new(BTreeMap::from([("job".into(), parent.to_path_buf())]))?;
    let job: ProfileJob = json(
        &jobs.read(
            "job",
            path.file_name()
                .unwrap()
                .to_str()
                .ok_or_else(|| invalid("Job filename"))?,
            JSON_LIMIT,
        )?,
    )?;
    if job.schema != "ddi-danoni-native-profile-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let source = DirectorySource::new(
        job.roots
            .into_iter()
            .map(|(k, p)| (k, if p.is_absolute() { p } else { parent.join(p) }))
            .collect(),
    )?;
    execute(&job.request, &worker, &source, output, progress)
}
