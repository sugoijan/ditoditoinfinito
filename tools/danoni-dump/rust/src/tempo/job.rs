//! Source-bound estimation and explicitly versioned cache/legacy bridges.
use super::*;
use crate::{
    assets::{self, AssetSource, Binding, Inputs, JSON_LIMIT, MEDIA_LIMIT, sha256},
    audio,
    model::PreparationOptions,
    work,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisPcm {
    pub source_audio_sha256: String,
    pub pcm: Binding,
    pub receipt: Binding,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TempoRequest {
    pub manifest: Binding,
    #[serde(default)]
    pub media: Vec<AnalysisPcm>,
    pub legacy_analysis: Option<Binding>,
    pub cached: Option<Binding>,
}
fn authored(entries: &[&crate::reviews::Entry]) -> Option<AuthoredRange> {
    let mut range = None::<(f64, f64)>;
    for e in entries {
        let s = e.metadata.get("declaredDisplayBpm")?.as_str()?;
        let parts = s.split(['-', ':']).collect::<Vec<_>>();
        if parts.is_empty() || parts.len() > 2 {
            return None;
        }
        let a = parts[0].parse::<f64>().ok()?;
        let b = parts.last()?.parse::<f64>().ok()?;
        if a <= 0.0 || a > b || !a.is_finite() || !b.is_finite() {
            return None;
        }
        if range.is_some_and(|r| r != (a, b)) {
            return None;
        }
        range = Some((a, b));
    }
    range.map(|(minimum_bpm, maximum_bpm)| AuthoredRange {
        minimum_bpm,
        maximum_bpm,
    })
}
fn verify_grids(entries: &[crate::reviews::Entry], review: &Value) -> Result<Value> {
    let rows = review["charts"]
        .as_object()
        .ok_or_else(|| invalid("Missing grid rows"))?;
    if rows.len() != entries.len()
        || rows
            .keys()
            .any(|k| !entries.iter().any(|e| e.chart.id == *k))
    {
        return Err(invalid("Cached grid chart coverage differs"));
    }
    let (mut endpoints, mut maximum, mut rhythmic) = (0usize, 0f64, 0usize);
    for e in entries {
        let grid: MusicalGrid = serde_json::from_value(rows[&e.chart.id].clone())
            .map_err(|e| invalid(e.to_string()))?;
        let c = crate::compile_chart(
            &e.chart,
            &e.metadata,
            &PreparationOptions {
                grid: Some(grid),
                ..Default::default()
            },
        )?;
        endpoints += c.endpoints.len();
        maximum = maximum.max(c.maximum_note_error_ms);
        rhythmic += c
            .endpoints
            .iter()
            .filter(|e| {
                matches!(
                    e.method,
                    crate::model::QuantizationMethod::RhythmicSubdivision
                )
            })
            .count();
    }
    Ok(
        json!({"sourceEndpoints":endpoints,"maximumNoteErrorMs":maximum,"rhythmicEndpoints":rhythmic,"allLaneRelationshipsAndSpeedEventsChecked":true}),
    )
}
/// Legacy analysis is an explicit compatibility input, never mistaken for fresh
/// native onsets. The source manifest, source music and chart coverage must agree.
pub fn prepare_tempo(
    request: &TempoRequest,
    source: &impl AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    checkpoint()?;
    let mut inputs = Inputs::new(source);
    let raw = inputs.checked(&request.manifest, JSON_LIMIT)?;
    let doc: Value = assets::json(&raw)?;
    let entries = work::entries(&doc, &request.manifest, &mut inputs)?;
    let mut groups = BTreeMap::<String, Vec<&crate::reviews::Entry>>::new();
    for e in &entries {
        crate::timing::validate_source_chart(&e.chart)?;
        let _ = distinct_heads(&e.chart)?;
        groups.entry(e.music.sha256.clone()).or_default().push(e);
    }
    if request.legacy_analysis.is_some() && !request.media.is_empty() {
        return Err(invalid(
            "Choose native PCM analysis or an explicit legacy bridge",
        ));
    }
    let legacy = if let Some(b) = &request.legacy_analysis {
        let bytes = inputs.checked(b, JSON_LIMIT)?;
        let legacy: Value = assets::json(&bytes)?;
        if legacy["schema"] != "ddi-tempo-analysis/experimental-1"
            || legacy["sourceManifestSha256"] != request.manifest.sha256
        {
            return Err(invalid("Legacy analysis source binding differs"));
        }
        Some(legacy)
    } else {
        None
    };
    let mut pcms = BTreeMap::new();
    for m in &request.media {
        if !groups.contains_key(&m.source_audio_sha256) || pcms.contains_key(&m.source_audio_sha256)
        {
            return Err(invalid("Unknown/repeated analysis PCM"));
        }
        let bytes = inputs.checked(&m.pcm, MEDIA_LIMIT)?;
        let proof: Value = assets::json(&inputs.checked(&m.receipt, JSON_LIMIT)?)?;
        if proof["schema"] != "ddi-danoni-prepared-media/1"
            || proof["sourceManifestSha256"] != request.manifest.sha256
            || proof["sourceAudioSha256"] != m.source_audio_sha256
            || proof["pcmSha256"] != m.pcm.sha256
        {
            return Err(invalid("Analysis PCM receipt does not bind source"));
        }
        crate::preview::pcm_envelope(&bytes, checkpoint)?;
        let decoded = audio::decode(&bytes, &audio::DecodeOptions::default(), checkpoint)?;
        if proof["sampleRate"] != decoded.pcm.rate
            || proof["channels"] != decoded.pcm.channels
            || proof["sampleFrames"] != decoded.pcm.frames()
        {
            return Err(invalid("Analysis PCM dimensions differ"));
        }
        pcms.insert(m.source_audio_sha256.clone(), decoded.pcm);
    }
    if legacy.is_none() && pcms.len() != groups.len() {
        return Err(invalid("Need one proved PCM per source audio hash"));
    }
    let policy = if legacy.is_some() {
        "ddi-tempo-selection/1"
    } else {
        "ddi-tempo-selection/2"
    };
    let mut identity = json!({"schema":"ddi-danoni-tempo-cache-key/1","sourceManifestSha256":request.manifest.sha256,"policy":policy,"lattice":"ddi-chart-lattice/1","onsets":if legacy.is_some(){"legacy-analysis-bridge/1"}else{"ddi-spectral-flux-dp/3"},"dependencies":{"rustfft":"6.4.1","rubato":"5.0.1","symphonia":"0.6.1"},"legacyAnalysis":request.legacy_analysis,"media":request.media,"verifiedSourceInputs":inputs.used});
    if legacy.is_none() {
        identity["quarterPhase"] = json!("ddi-quarter-onset-support/1");
    }
    let key = sha256(&serde_json::to_vec(&identity).map_err(|e| invalid(e.to_string()))?);
    if let Some(binding) = &request.cached {
        let cached: Value = assets::json(&inputs.checked(binding, JSON_LIMIT)?)?;
        if cached["schema"] != "ddi-danoni-tempo-result/1"
            || cached["cacheKey"] != key
            || cached["sourceManifestSha256"] != request.manifest.sha256
            || cached["cacheIdentity"] != identity
        {
            return Err(invalid("Tempo cache identity changed"));
        }
        if cached["status"] == "estimated" {
            if cached["review"]["schema"] != "ddi-reviewed-musical-grid/experimental-1"
                || cached["review"]["sourceManifestSha256"] != request.manifest.sha256
            {
                return Err(invalid("Cached review source differs"));
            }
            verify_grids(&entries, &cached["review"])?;
        } else if cached["status"] != "exact-source-fallback" {
            return Err(invalid("Unknown cached tempo outcome"));
        }
        return Ok(cached);
    }
    let mut tracks = Vec::new();
    let mut review_charts = serde_json::Map::new();
    let mut failure = None::<Value>;
    if let Some(legacy) = &legacy {
        let rows = legacy["tracks"]
            .as_array()
            .ok_or_else(|| invalid("Missing legacy analysis tracks"))?;
        if rows.len() != groups.len() {
            return Err(invalid("Legacy music coverage differs"));
        }
    }
    for (hash, group) in groups {
        checkpoint()?;
        let charts = group
            .iter()
            .map(|e| Ok((e.chart.id.clone(), distinct_heads(&e.chart)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut times = charts.values().flatten().copied().collect::<Vec<_>>();
        times.sort_by(f64::total_cmp);
        times.dedup();
        let range = authored(&group);
        let mut onset = None;
        let mut phase_report = None;
        let mut windows = Vec::new();
        let mut analysis = if let Some(legacy) = &legacy {
            let selected = legacy["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["audioSha256"] == hash)
                .collect::<Vec<_>>();
            if selected.len() != 1 {
                return Err(invalid("Legacy audio hash coverage differs"));
            }
            let row = selected[0];
            let ids = row["chartIds"]
                .as_array()
                .ok_or_else(|| invalid("Missing legacy chart identities"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| invalid("Invalid legacy chart identity"))
                })
                .collect::<Result<std::collections::BTreeSet<_>>>()?;
            if ids != charts.keys().map(|k| k.as_str()).collect() {
                return Err(invalid("Legacy chart/music grouping differs"));
            }
            let mut a: TrackAnalysis =
                serde_json::from_value(row.clone()).map_err(|e| invalid(e.to_string()))?;
            if a.dominant_grid["status"] == "dominant-portions-only" {
                let w: Vec<ChartWindow> = serde_json::from_value(row["chartWindows"].clone())
                    .map_err(|e| invalid(e.to_string()))?;
                for window in &w {
                    let subset = times
                        .iter()
                        .copied()
                        .filter(|t| window.start_seconds <= *t && *t < window.end_seconds)
                        .collect::<Vec<_>>();
                    let f = chart_fit(&subset, window.fit.bpm, window.fit.subdivisions_per_beat)?;
                    if (f.coherence - window.fit.coherence).abs() > 1e-8
                        || subset.len() != window.notes
                    {
                        return Err(invalid(
                            "Legacy stable-window fit differs from current heads",
                        ));
                    }
                }
                a.dominant_grid = dominant_grid(
                    &times,
                    &w,
                    a.dominant_grid["windowBpmTolerance"]
                        .as_f64()
                        .ok_or_else(|| invalid("Missing window tolerance"))?,
                    checkpoint,
                )?;
                windows = w;
            }
            a
        } else {
            let report = onset_estimates(&pcms[&hash], checkpoint)?;
            let candidates = chart_candidates(&times, checkpoint);
            match candidates {
                Ok(candidates) => {
                    let seed = candidates
                        .first()
                        .ok_or_else(|| insufficient("No lattice candidate"))?;
                    windows = local_windows(&times, seed, checkpoint)?;
                    let dominant = dominant_grid(&times, &windows, seed.bpm * 0.0015, checkpoint)?;
                    let mut body =
                        serde_json::to_value(&report).map_err(|e| invalid(e.to_string()))?;
                    let env = body.as_object_mut().unwrap().remove("envelope").unwrap();
                    body["envelopeFrames"] = json!(report.envelope.len());
                    body["envelopeSha256"] = json!(sha256(
                        &serde_json::to_vec(&env).map_err(|e| invalid(e.to_string()))?
                    ));
                    onset = Some(body);
                    phase_report = Some(report.clone());
                    TrackAnalysis {
                        chart_candidates: candidates,
                        audio_estimates: report.estimates,
                        dominant_grid: dominant,
                        authored_bpm_range: range.clone(),
                    }
                }
                Err(e) if e.code == "insufficient-tempo" => {
                    failure = Some(json!(e));
                    TrackAnalysis {
                        chart_candidates: vec![],
                        audio_estimates: report.estimates,
                        dominant_grid: Value::Null,
                        authored_bpm_range: range.clone(),
                    }
                }
                Err(e) => return Err(e),
            }
        };
        analysis.authored_bpm_range = range;
        let selection = if legacy.is_some() {
            select_grid(&times, &analysis)
        } else {
            select_native_grid(&times, &analysis)
        };
        match selection {
            Ok((grid, pulses)) => {
                let (grids, shared) = if charts.len() > 1 {
                    if legacy.is_some() {
                        select_shared(&charts, &analysis, &pulses, &grid, checkpoint)?
                    } else {
                        select_native_shared(&charts, &analysis, &pulses, &grid, checkpoint)?
                    }
                } else {
                    (
                        charts.keys().map(|id| (id.clone(), grid.clone())).collect(),
                        Value::Null,
                    )
                };
                for (id, mut g) in grids {
                    if let Some(report) = &phase_report {
                        resolve_quarter_phase(&mut g, report, &charts[&id], checkpoint)?;
                    }
                    review_charts.insert(id, json!(g));
                }
                tracks.push(json!({"sourceAudioSha256":hash,"chartIds":charts.keys().collect::<Vec<_>>(),"distinctHeads":times.len(),"analysis":analysis,"nativeOnsets":onset,"chartWindows":windows,"shared":shared}));
            }
            Err(e) if e.code == "insufficient-tempo" => {
                failure = Some(json!(e));
                tracks.push(json!({"sourceAudioSha256":hash,"chartIds":charts.keys().collect::<Vec<_>>(),"analysis":analysis,"nativeOnsets":onset,"reason":e}));
            }
            Err(e) => return Err(e),
        }
    }
    let mut review = json!({"schema":"ddi-reviewed-musical-grid/experimental-1","sourceManifestSha256":request.manifest.sha256,"charts":review_charts,"origin":"Rust estimated grid; every source endpoint and speed event checked","algorithm":policy,"cacheKey":key});
    let mut validation = Value::Null;
    if failure.is_none() {
        match verify_grids(&entries, &review) {
            Ok(v) => validation = v,
            Err(e) if ["timing-budget-exceeded", "unrepresentable-lane"].contains(&e.code) => {
                failure = Some(json!(e))
            }
            Err(e) => return Err(e),
        }
    }
    if failure.is_some() {
        review = Value::Null;
    }
    Ok(
        json!({"schema":"ddi-danoni-tempo-result/1","status":if failure.is_none(){"estimated"}else{"exact-source-fallback"},"sourceManifestSha256":request.manifest.sha256,"cacheKey":key,"cacheIdentity":identity,"review":review,"tracks":tracks,"validation":validation,"fallback":failure,"fallbackPolicy":"No partial musical remapping; preserve the source clock","sourceNotesModified":false,"producer":if legacy.is_some(){"Rust decision policy over explicit preserved legacy analysis"}else{"Native Rust audio/lattice/decision algorithms; independent of librosa"}}),
    )
}
