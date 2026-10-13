//! Join passive source/run/geometry/PCM proofs into a portable normalized work.
use super::*;
use crate::{
    assets::{
        self, Artifact, AssetSource, Binding, Inputs, JSON_LIMIT, MEDIA_LIMIT, RelativeBinding,
        sha256,
    },
    audio::{self, DecodeOptions, Pcm16},
    model::Metadata,
    swf,
};
use std::{collections::BTreeSet, sync::Arc};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeChartRequest {
    pub id: String,
    pub label: Option<String>,
    pub options: ProjectionOptions,
    pub projection: CaptureBindings,
    pub motion: CaptureBindings,
    pub motion_selection: MotionSelection,
    pub audio: CaptureBindings,
    pub native_pcm: Binding,
    pub stream_start_index: usize,
    pub optional_color: Option<OptionalColorEvidence>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeWorkRequest {
    pub source_manifest: Binding,
    pub movie: Binding,
    #[serde(default)]
    pub metadata: Metadata,
    pub charts: Vec<NativeChartRequest>,
}
pub struct NormalizedWork {
    pub manifest: Value,
    pub receipt: Value,
    pub files: Vec<Artifact>,
}
fn rejected(code: &'static str, message: impl Into<String>) -> Error {
    Error::new(code, message)
}
fn json_bytes(v: &Value) -> Result<Arc<Vec<u8>>> {
    let mut b = serde_json::to_vec_pretty(v).map_err(|e| invalid(e.to_string()))?;
    b.push(b'\n');
    if b.len() > JSON_LIMIT {
        return Err(rejected(
            "normalization-limit",
            "Generated JSON exceeds 16 MiB",
        ));
    }
    Ok(Arc::new(b))
}
fn add(files: &mut BTreeMap<String, Artifact>, path: String, bytes: Arc<Vec<u8>>) -> Result<()> {
    assets::relative_path(&path)?;
    if let Some(old) = files.get(&path) {
        if old.bytes != bytes {
            return Err(invalid("Conflicting normalized artifact"));
        }
    } else {
        if files.len() >= 32768
            || files
                .values()
                .map(|f| f.bytes.len())
                .sum::<usize>()
                .checked_add(bytes.len())
                .is_none_or(|n| n > 512 * 1024 * 1024)
        {
            return Err(rejected(
                "normalization-limit",
                "Normalized artifacts exceed 512 MiB / 32768 files",
            ));
        }
        files.insert(path.clone(), Artifact { path, bytes });
    }
    Ok(())
}
fn add_json(files: &mut BTreeMap<String, Artifact>, path: String, v: &Value) -> Result<String> {
    let b = json_bytes(v)?;
    let hash = sha256(&b);
    add(files, path, b)?;
    Ok(hash)
}
/// Run observation/renderer/mixer/budget settings may differ. Script input,
/// selector events, immutable resource declarations and every other setting may not.
fn controls(config: &Value) -> Result<Value> {
    let mut c = config
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("Invalid runtime config"))?;
    for k in ["captureFrames", "render", "mixAudio", "avmExecutionMs"] {
        c.remove(k);
    }
    Ok(Value::Object(c))
}
fn run_identity(run: &Value) -> Result<Value> {
    if !["workerSha256", "sandboxSha256"]
        .iter()
        .all(|k| run[*k].as_str().is_some_and(assets::valid_hash))
    {
        return Err(invalid("Missing worker/sandbox receipt identities"));
    }
    Ok(json!({"workerSha256":run["workerSha256"],"sandboxSha256":run["sandboxSha256"]}))
}
fn verify_inputs(
    inputs: &mut Inputs<'_, impl AssetSource>,
    binding: &CaptureBindings,
    capture: &Value,
    movie: &Binding,
) -> Result<Value> {
    let config = &capture["source"]["config"];
    let name = config["movie"]
        .as_str()
        .ok_or_else(|| invalid("Missing configured movie"))?;
    let resolved = binding.config.sibling(&RelativeBinding {
        path: name.into(),
        sha256: movie.sha256.clone(),
    })?;
    if resolved.namespace != movie.namespace || resolved.path != movie.path {
        return Err(invalid("Configured movie binding differs"));
    }
    let resources = config["resources"]
        .as_array()
        .ok_or_else(|| invalid("Missing immutable resource inventory"))?;
    if resources.len() > 4096 {
        return Err(invalid("Resource inventory limit"));
    }
    let mut urls = BTreeSet::new();
    for resource in resources {
        let url = resource["url"]
            .as_str()
            .ok_or_else(|| invalid("Invalid resource URL"))?;
        if !urls.insert(url) {
            return Err(invalid("Duplicate immutable resource URL"));
        }
        let relative: RelativeBinding =
            serde_json::from_value(json!({"path":resource["path"],"sha256":resource["sha256"]}))
                .map_err(|e| invalid(e.to_string()))?;
        inputs.checked(&binding.config.sibling(&relative)?, MEDIA_LIMIT)?;
    }
    let run: Value = assets::json(&inputs.checked(&binding.run, JSON_LIMIT)?)?;
    run_identity(&run)
}
/// Compare complete error multiplicity/iteration/details, not merely membership.
/// The earlier projector itself proves compilation and required receptors.
pub(super) fn joined_diagnostics(projection: &Value, capture: &Value) -> Result<Vec<Value>> {
    let diagnostics = projection
        .get("presentationDiagnostics")
        .map(array)
        .transpose()?
        .cloned()
        .unwrap_or_default();
    if !diagnostics.is_empty() {
        let errors = array(&capture["audit"]["entries"])?
            .iter()
            .filter(|e| e["kind"] == "runtime-error")
            .cloned()
            .collect::<Vec<_>>();
        if errors != diagnostics {
            return Err(rejected(
                "native-decoration-proof-mismatch",
                "Cross-run decoration errors differ in count, order, iteration or detail",
            ));
        }
    }
    audit_ok(capture, &diagnostics)?;
    Ok(diagnostics)
}
fn native_pcm(bytes: &[u8], capture: &Value) -> Result<Pcm16> {
    let audio = &capture["audio"];
    let frames = audio["sampleFrames"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 44100 * 600)
        .ok_or_else(|| invalid("Invalid native PCM frame count"))? as usize;
    if audio["sampleRate"] != 44100
        || audio["channels"] != 2
        || bytes.len() != frames * 4
        || capture["source"]["config"]["mixAudio"] != true
    {
        return Err(invalid(
            "Native PCM dimensions/mixer configuration differ from capture",
        ));
    }
    let pcm = Pcm16 {
        rate: 44100,
        channels: 2,
        samples: bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| i16::from_le_bytes(*v))
            .collect(),
    };
    pcm.validate()?;
    Ok(pcm)
}
fn effects(p: &Value, optional: Option<(&Value, &[Value])>) -> Result<Value> {
    let mut rows = vec![];
    if p["inactiveSpeedTail"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        rows.push(json!({"type":"inactive-native-speed-tail","values":p["inactiveSpeedTail"]}));
    }
    if p["motionSpeedPolicy"] == "per-arrow-at-spawn" {
        rows.push(json!({"type":"per-arrow-speed-approximation","nativeSpeedChanges":p["nativeSpeedChanges"],"scope":"SSC global scroll approximates source per-arrow spawn speeds; note timestamps are preserved"}));
    }
    if let Some((review, diagnostics)) = optional {
        rows.push(
            json!({"type":"missing-optional-color","review":review,"diagnostics":diagnostics}),
        );
    }
    if p["nativePresentation"]["iro_dac"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        rows.push(json!({"type":"unmapped-native-presentation","data":p["nativePresentation"]}));
    }
    if ["taps", "holds"].iter().any(|k| {
        p["inactiveSchedules"][*k]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    }) {
        rows.push(json!({"type":"inactive-native-lane-schedules","data":p["inactiveSchedules"]}));
    }
    for (index, lane) in array(&p["lanes"])?.iter().enumerate() {
        if let Some(issues) = lane.get("sourceOrderIssues") {
            for issue in array(issues)? {
                let mut r = issue
                    .as_object()
                    .cloned()
                    .ok_or_else(|| invalid("Invalid source cursor issue"))?;
                r.insert("type".into(), json!("source-cursor-order"));
                r.insert("lane".into(), json!(index));
                rows.push(Value::Object(r));
            }
        }
    }
    Ok(json!(rows))
}
/// No subprocess/browser/network/work-code execution. Every requested chart is
/// required; any proof failure rejects the work without returning partial files.
pub fn prepare_native_work(
    request: &NativeWorkRequest,
    source: &impl AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<NormalizedWork> {
    checkpoint()?;
    if request.charts.is_empty()
        || request.charts.len() > 128
        || request.metadata.len() > 128
        || request
            .metadata
            .iter()
            .any(|(k, v)| k.len() > 128 || !v.as_str().is_some_and(|s| s.len() <= 8192))
    {
        return Err(invalid("Need bounded chart selection and text metadata"));
    }
    let mut ids = BTreeSet::new();
    for c in &request.charts {
        if c.id.is_empty()
            || c.id.len() > 128
            || !c
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            || !ids.insert(&c.id)
            || c.label.as_ref().is_some_and(|s| s.len() > 8192)
        {
            return Err(invalid("Invalid/duplicate chart identity or label"));
        }
    }
    let mut inputs = Inputs::new(source);
    let source_bytes = inputs.checked(&request.source_manifest, JSON_LIMIT)?;
    let original_source: Value = assets::json(&source_bytes)?;
    let source_hash = original_source
        .get("sourceSwfSha256")
        .unwrap_or(&original_source["swf"]["sha256"]);
    if source_hash.as_str() != Some(request.movie.sha256.as_str()) {
        return Err(invalid("Source manifest belongs to another movie"));
    }
    let original = inputs.checked(&request.movie, swf::MOVIE_LIMIT)?;
    let movie = swf::Movie::read(&original, checkpoint)?;
    if movie.header.fps != 60.0 {
        return Err(invalid("Normalized families require 60 Hz"));
    }
    let mut files = BTreeMap::new();
    add(&mut files, "source-manifest.json".into(), source_bytes)?;
    add_json(
        &mut files,
        "request.json".into(),
        &serde_json::to_value(request).map_err(|e| invalid(e.to_string()))?,
    )?;
    let mut charts = vec![];
    let mut motion_reviews = BTreeMap::new();
    let mut proofs = vec![];
    let mut decoded = BTreeMap::<String, audio::Decoded>::new();
    let mut shared = BTreeMap::<String, (f64, String)>::new();
    let mut captured_at = Value::Null;
    for row in &request.charts {
        checkpoint()?;
        let optional = row.optional_color.as_ref();
        let (projection_capture, optional_diagnostics) =
            checked_capture(&row.projection, &request.movie, optional, &mut inputs)?;
        let (motion_capture, motion_optional) =
            checked_capture(&row.motion, &request.movie, optional, &mut inputs)?;
        let (audio_capture, audio_optional) =
            checked_capture(&row.audio, &request.movie, optional, &mut inputs)?;
        let identity = verify_inputs(
            &mut inputs,
            &row.projection,
            &projection_capture,
            &request.movie,
        )?;
        for (binding, capture) in [(&row.motion, &motion_capture), (&row.audio, &audio_capture)] {
            if verify_inputs(&mut inputs, binding, capture, &request.movie)? != identity {
                return Err(rejected(
                    "native-run-identity-mismatch",
                    "Worker/sandbox identities differ between runs",
                ));
            }
            if controls(&capture["source"]["config"])?
                != controls(&projection_capture["source"]["config"])?
            {
                return Err(rejected(
                    "native-run-controls-mismatch",
                    "Selector events or script/resource settings differ between runs",
                ));
            }
        }
        let projection = project_capture(&projection_capture, &row.options, checkpoint)?;
        let motion_diagnostics = joined_diagnostics(&projection.chart, &motion_capture)?;
        let audio_diagnostics = joined_diagnostics(&projection.chart, &audio_capture)?;
        let clock = projection.chart["clockField"].as_str();
        let motion = movie_observation(
            &motion_capture,
            &projection.chart,
            &row.motion_selection,
            clock,
            checkpoint,
        )?;
        let origin = movie_audio_origin(&audio_capture, clock, row.stream_start_index)?;
        validate_clock_agreement(&origin, &motion)?;
        let demux = swf::select_capture(
            &movie,
            &audio_capture,
            row.stream_start_index,
            &audio_diagnostics,
            checkpoint,
        )?;
        let mp3_hash = sha256(&demux.bytes);
        if !decoded.contains_key(&mp3_hash) {
            decoded.insert(
                mp3_hash.clone(),
                audio::decode(&demux.bytes, &DecodeOptions::default(), checkpoint)?,
            );
        }
        let d = &decoded[&mp3_hash];
        let native_bytes = inputs.checked(&row.native_pcm, MEDIA_LIMIT)?;
        let native = native_pcm(&native_bytes, &audio_capture)?;
        let (comparison, resample) = audio::resample(&d.pcm, 44100, true, checkpoint)?;
        let start = origin["outputSampleFrame"]
            .as_u64()
            .ok_or_else(|| invalid("Invalid observed output sample frame"))?
            as usize;
        let end = origin
            .get("nextStreamOutputSampleFrame")
            .map(|v| {
                v.as_u64()
                    .map(|n| n as usize)
                    .ok_or_else(|| invalid("Invalid next stream output frame"))
            })
            .transpose()?;
        let matched = audio::origin_match(&native, &comparison, start, end, checkpoint)?;
        let shift =
            (matched.lag_samples as f64 * d.pcm.rate as f64 / 44100.0).round_ties_even() as i64;
        let canonical = d.pcm.shifted(shift)?.wav()?;
        let canonical_hash = sha256(&canonical);
        let logical = numeric(&origin["logicalFrame"])?;
        if let Some((first_origin, first_hash)) = shared.get(&mp3_hash) {
            if *first_origin != logical || *first_hash != canonical_hash {
                return Err(rejected(
                    "native-shared-audio-origin-mismatch",
                    "Shared MP3 charts have different logical or measured decoder origins",
                ));
            }
        } else {
            shared.insert(mp3_hash.clone(), (logical, canonical_hash.clone()));
        }
        let directory = format!("media/{mp3_hash}");
        add(
            &mut files,
            format!("{directory}/source.mp3"),
            Arc::new(demux.bytes),
        )?;
        add(
            &mut files,
            format!("{directory}/source-decoded.wav"),
            Arc::new(d.pcm.wav()?),
        )?;
        add(
            &mut files,
            format!("{directory}/music.wav"),
            Arc::new(canonical),
        )?;
        // A separate stream report per chart retains distinct timeline slices.
        add_json(
            &mut files,
            format!("references/{}-stream.json", row.id),
            &demux.report,
        )?;
        let audio_report = json!({"schema":"ddi-danoni-native-audio-origin/1","producer":"native-origin-normalization/1","sourceMp3Sha256":mp3_hash,"nativePcmSha256":row.native_pcm.sha256,"originalFrames":d.pcm.frames(),"sampleRate":d.pcm.rate,"channels":d.pcm.channels,"appliedShiftSamples":shift,"canonicalSha256":canonical_hash,"decode":d.report,"resample":resample,"originMatch":matched,"scope":"Measured decoder-origin correction against bound native PCM; no musical offset or original Flash/device latency estimate"});
        add_json(
            &mut files,
            format!("references/{}-audio-origin.json", row.id),
            &audio_report,
        )?;
        let mut metadata = request.metadata.clone();
        if !metadata
            .get("artist")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            if let Some(artist) = projection.chart["metadata"]["ARTIST_NAME"]
                .as_str()
                .and_then(|s| s.strip_prefix("Artist"))
                .map(str::trim)
                .filter(|s| !["", "undefined", "null"].contains(s))
            {
                metadata.insert("artist".into(), json!(artist));
            } else if let Some(artist) = original_source["artist"].as_str() {
                metadata.insert("artist".into(), json!(artist));
            }
        }
        metadata.insert(
            "difficultyLabel".into(),
            json!(
                row.label
                    .as_deref()
                    .unwrap_or_else(|| projection.chart["metadata"]["diffi"]
                        .as_str()
                        .unwrap_or(""))
            ),
        );
        let optional_review = optional
            .map(|o| {
                inputs
                    .checked(&o.review, JSON_LIMIT)
                    .and_then(|b| assets::json::<Value>(&b))
            })
            .transpose()?;
        let mut chart = relative_chart(&projection.chart, logical)?;
        chart["id"] = json!(row.id);
        chart["music"] = json!({"path":format!("{directory}/music.wav"),"sha256":canonical_hash,"mimeType":"audio/wav"});
        chart["audioOrigin"] = origin.clone();
        chart["nativeReference"] = json!(format!("references/{}.json", row.id));
        chart["metadata"] = json!(metadata);
        chart["difficultyStars"] = Value::Null;
        chart["effects"] = effects(
            &projection.chart,
            optional_review
                .as_ref()
                .map(|v| (v, optional_diagnostics.as_slice())),
        )?;
        let reference = json!({"schema":"ddi-native-family-reference/experimental-1","family":row.options.family,"sourceSwfSha256":request.movie.sha256,"preparedIteration":projection.prepared_iteration,"clock":clock.unwrap_or("root movie frame"),"audioOrigin":origin,"projection":projection.chart,"motion":motion,"metadata":metadata,"sourceSnapshotSha256":row.projection.snapshot.sha256,"normalizationPolicy":"native-normalization/1","limits":["Source frame data preserved; original Flash/device latency not measured","Frame-phase differences up to one native tick remain observable in original rendering","Unsorted source arrays and native presentation diagnostics are retained; arbitrary engine customization is not reproduced"]});
        add_json(
            &mut files,
            format!("references/{}.json", row.id),
            &reference,
        )?;
        let motion_path = format!("references/{}-motion.json", row.id);
        let motion_hash = add_json(&mut files, motion_path.clone(), &motion)?;
        motion_reviews.insert(row.id.clone(),json!({"baseArrowHeightsPerSecond":motion["baseArrowHeightsPerSecond"],"evidence":{"path":motion_path,"sha256":motion_hash},"scope":motion["scope"]}));
        let run: Value = assets::json(&inputs.checked(&row.projection.run, JSON_LIMIT)?)?;
        if captured_at.is_null() {
            captured_at = run["startedAt"].clone();
        }
        proofs.push(json!({"id":row.id,"captures":{"projection":row.projection,"motion":row.motion,"audio":row.audio},"worker":identity,"preparedIteration":projection.prepared_iteration,"sourceMp3Sha256":mp3_hash,"canonicalSha256":canonical_hash,"nativePcm":row.native_pcm,"presentationDiagnostics":{"motion":motion_diagnostics,"audio":audio_diagnostics},"optionalColorDiagnostics":{"projection":optional_diagnostics,"motion":motion_optional,"audio":audio_optional},"logicalClockAgreement":true,"independentDecoderOrigin":audio_report}));
        charts.push(chart);
    }
    let page = original_source
        .get("pageUrl")
        .unwrap_or(&original_source["page"]["url"]);
    let manifest = json!({"schema":"ddi-normalized-work/experimental-1","adapter":"native-rust-families/1","sourceManifestSha256":request.source_manifest.sha256,"sourceSwfSha256":request.movie.sha256,"sourcePageUrl":page,"capturedAt":captured_at,"metadata":request.metadata,"charts":charts,"normalization":"normalization.json","assessment":{"nativeReference":"source/schedule, source-bound control runs and movement cross-checked","audio":"demuxed observed continuous slice; independent bound native PCM origins matched","conversion":"requires musical review and game-parser verification before promotion","sourceData":"local only"}});
    let manifest_hash = add_json(&mut files, "manifest.json".into(), &manifest)?;
    add_json(
        &mut files,
        "native-scroll-review.json".into(),
        &json!({"schema":"ddi-reviewed-native-scroll/experimental-1","sourceManifestSha256":manifest_hash,"charts":motion_reviews}),
    )?;
    let mut preserved = BTreeMap::new();
    for (key, binding) in inputs.used.clone() {
        checkpoint()?;
        let bytes = inputs.checked(&binding, MEDIA_LIMIT)?;
        let path = format!("evidence/inputs/{}", binding.sha256);
        add(&mut files, path.clone(), bytes)?;
        preserved.insert(key, json!({"binding":binding,"path":path}));
    }
    let receipt = json!({"schema":"ddi-danoni-native-normalization/1","producer":"native-normalization/1","request":request,"sourceInputs":preserved,"charts":proofs,"normalizedManifestSha256":manifest_hash,"artifacts":files.values().map(|f|json!({"path":f.path,"sha256":sha256(&f.bytes),"bytes":f.bytes.len()})).collect::<Vec<_>>(),"scope":"Passive successful saved-run extraction with measured PCM alignment; no fresh worker execution, acquisition, musical BPM fitting or library installation"});
    add_json(&mut files, "normalization.json".into(), &receipt)?;
    checkpoint()?;
    Ok(NormalizedWork {
        manifest,
        receipt,
        files: files.into_values().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding(path: &str) -> Binding {
        Binding {
            namespace: "test".into(),
            path: path.into(),
            sha256: "0".repeat(64),
        }
    }
    fn row() -> NativeChartRequest {
        let c = CaptureBindings {
            snapshot: binding("snapshot"),
            run: binding("run"),
            config: binding("config"),
        };
        NativeChartRequest {
            id: "normal".into(),
            label: None,
            options: ProjectionOptions {
                family: Family::ChildTimeline5,
                mode: crate::model::Mode::Five,
                source_prefix: String::new(),
                source_suffix: None,
                review_missing_decorative_sprites: false,
            },
            projection: c.clone(),
            motion: c.clone(),
            audio: c,
            native_pcm: binding("pcm"),
            stream_start_index: 0,
            optional_color: None,
            motion_selection: MotionSelection {
                note: "arrow".into(),
                receptor: "target".into(),
                lane: 0,
                iterations: vec![1, 2, 3],
                target_y: None,
                geometry_reference: None,
            },
        }
    }
    struct NoReads;
    impl AssetSource for NoReads {
        fn read(&self, _: &str, _: &str, _: usize) -> Result<Vec<u8>> {
            panic!("Invalid/canceled selection must not read assets")
        }
    }
    #[test]
    fn malformed_chart_selections_and_cancellation_do_not_read_assets() {
        let mut request = NativeWorkRequest {
            source_manifest: binding("source"),
            movie: binding("movie"),
            metadata: Metadata::new(),
            charts: vec![row(), row()],
        };
        assert!(prepare_native_work(&request, &NoReads, &mut || Ok(())).is_err());
        request.charts.truncate(1);
        request
            .metadata
            .insert("title".into(), json!({"unexpected":"object"}));
        assert!(prepare_native_work(&request, &NoReads, &mut || Ok(())).is_err());
        request.metadata.clear();
        request.charts[0].id = "../escape".into();
        assert!(prepare_native_work(&request, &NoReads, &mut || Ok(())).is_err());
        let error = prepare_native_work(&request, &NoReads, &mut || {
            Err(Error::new("cancelled", "test"))
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "cancelled");
    }
    #[test]
    fn run_controls_allow_instrumentation_but_retain_script_and_selector_identity() {
        let config = json!({"movie":"blobs/1","sourceUrl":"https://example.test/work.swf","events":[{"type":"enter","frame":10}],"resources":[{"url":"https://example.test/chart.txt","sha256":"1"}],"captureFrames":[10],"render":false,"mixAudio":false,"avmExecutionMs":5000});
        let mut changed = config.clone();
        changed["captureFrames"] = json!([100, 101]);
        changed["render"] = json!(true);
        changed["mixAudio"] = json!(true);
        changed["avmExecutionMs"] = json!(10000);
        assert_eq!(controls(&config).unwrap(), controls(&changed).unwrap());
        for (key, value) in [
            ("events", json!([])),
            ("sourceUrl", json!("https://example.test/other.swf")),
            ("resources", json!([])),
            ("unknownScriptSetting", json!(2)),
        ] {
            let mut changed = config.clone();
            changed[key] = value;
            assert_ne!(controls(&config).unwrap(), controls(&changed).unwrap());
        }
    }
    #[test]
    fn joined_decoration_proofs_require_exact_error_multiplicity_iteration_and_audit() {
        let error = json!({"iteration":137,"kind":"runtime-error","detail":"message=Tried to instantiate a non-registered character undefined ","truncated":false});
        let projection = json!({"presentationDiagnostics":[error.clone(),error.clone()]});
        let capture = json!({"avm1Halted":false,"source":{"config":{"resources":[]}},"audit":{"truncated":false,"droppedEntries":0,"truncatedDetails":0,"runtimeErrors":2,"entries":[error.clone(),error.clone()]}});
        assert_eq!(joined_diagnostics(&projection, &capture).unwrap().len(), 2);
        let mut c = capture.clone();
        c["audit"]["entries"]
            .as_array_mut()
            .unwrap()
            .push(error.clone());
        c["audit"]["runtimeErrors"] = json!(3);
        assert_eq!(
            joined_diagnostics(&projection, &c).err().unwrap().code,
            "native-decoration-proof-mismatch"
        );
        let mut c = capture.clone();
        c["audit"]["entries"][0]["iteration"] = json!(138);
        assert!(joined_diagnostics(&projection, &c).is_err());
        let mut c = capture.clone();
        c["audit"]["truncated"] = json!(true);
        assert!(joined_diagnostics(&projection, &c).is_err());
        let mut c = capture.clone();
        c["audit"]["entries"][0]["detail"] = json!("Different runtime error");
        assert!(joined_diagnostics(&projection, &c).is_err());
        assert!(joined_diagnostics(&json!({}), &capture).is_err());
    }
    #[test]
    fn configured_resource_bodies_and_movie_paths_are_bound() {
        struct Memory(BTreeMap<String, Vec<u8>>);
        impl AssetSource for Memory {
            fn read(&self, _: &str, path: &str, _: usize) -> Result<Vec<u8>> {
                Ok(self.0[path].clone())
            }
        }
        let body = b"original synthetic dos text".to_vec();
        let report = json!({"workerSha256":"1".repeat(64),"sandboxSha256":"2".repeat(64)});
        let run = serde_json::to_vec(&report).unwrap();
        let memory = Memory(BTreeMap::from([
            ("run".into(), run.clone()),
            ("inputs/chart.txt".into(), body.clone()),
        ]));
        let bindings = CaptureBindings {
            snapshot: binding("snapshot"),
            run: Binding {
                namespace: "test".into(),
                path: "run".into(),
                sha256: sha256(&run),
            },
            config: binding("inputs/config.json"),
        };
        let movie = binding("inputs/movie.swf");
        let capture = json!({"source":{"config":{"movie":"movie.swf","resources":[{"url":"https://example.test/chart.txt","path":"chart.txt","sha256":sha256(&body)}]}}});
        let mut inputs = Inputs::new(&memory);
        assert_eq!(
            verify_inputs(&mut inputs, &bindings, &capture, &movie).unwrap(),
            report
        );
        assert!(inputs.used.contains_key("test:inputs/chart.txt"));
        let wrong_movie = binding("different/movie.swf");
        assert!(
            verify_inputs(&mut Inputs::new(&memory), &bindings, &capture, &wrong_movie).is_err()
        );
        let mut wrong = capture.clone();
        wrong["source"]["config"]["resources"][0]["sha256"] = json!("0".repeat(64));
        assert_eq!(
            verify_inputs(&mut Inputs::new(&memory), &bindings, &wrong, &movie)
                .err()
                .unwrap()
                .code,
            "changed-asset"
        );
        let mut duplicate = capture.clone();
        duplicate["source"]["config"]["resources"]
            .as_array_mut()
            .unwrap()
            .push(capture["source"]["config"]["resources"][0].clone());
        assert!(verify_inputs(&mut Inputs::new(&memory), &bindings, &duplicate, &movie).is_err());
    }
    #[test]
    fn native_pcm_requires_exact_captured_dimensions_and_enabled_mixer() {
        let c = json!({"source":{"config":{"mixAudio":true}},"audio":{"sampleRate":44100,"channels":2,"sampleFrames":2}});
        let bytes = [1, 0, 255, 255, 0, 128, 255, 127];
        assert_eq!(
            native_pcm(&bytes, &c).unwrap().samples,
            vec![1, -1, i16::MIN, i16::MAX]
        );
        assert!(native_pcm(&bytes[..7], &c).is_err());
        for (key, value) in [
            ("channels", json!(1)),
            ("sampleRate", json!(22050)),
            ("sampleFrames", json!(u64::MAX)),
        ] {
            let mut c = c.clone();
            c["audio"][key] = value;
            assert!(native_pcm(&bytes, &c).is_err());
        }
        let mut c = c;
        c["source"]["config"]["mixAudio"] = json!(false);
        assert!(native_pcm(&bytes, &c).is_err());
    }
}
