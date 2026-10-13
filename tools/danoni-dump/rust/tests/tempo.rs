use ddi_danoni_export::{audio::Pcm16, model::*, tempo::*, timing};
use serde_json::json;
use std::collections::BTreeMap;
fn times(bpm: f64, phase: f64, sub: u8, count: usize) -> Vec<f64> {
    (0..count)
        .map(|i| ((phase + i as f64 * 60.0 / bpm / sub as f64) * 60.0).round_ties_even() / 60.0)
        .collect()
}
fn chart(id: &str, ts: Vec<f64>) -> Chart {
    Chart {
        id: id.into(),
        mode: Mode::Five,
        lanes: (0..5)
            .map(|i| Lane {
                taps_seconds: if i == 0 { ts.clone() } else { vec![] },
                holds_seconds: vec![],
                source: Metadata::new(),
            })
            .collect(),
        speed: vec![],
        metadata: Metadata::new(),
        source: Metadata::new(),
    }
}
fn track(bpm: f64) -> TrackAnalysis {
    TrackAnalysis {
        chart_candidates: vec![],
        audio_estimates: vec![AudioEstimate {
            bpm,
            beat_times_seconds: (0..200).map(|i| 0.1 + i as f64 * 60.0 / bpm).collect(),
            tempo_prior_bpm: None,
        }],
        dominant_grid: json!(null),
        authored_bpm_range: None,
    }
}
#[test]
fn rounded_lattice_and_aliases_are_measured() {
    let ts = times(173.4, 0.137, 4, 180);
    let c = chart_candidates(&ts, &mut || Ok(())).unwrap();
    let f = c
        .iter()
        .filter(|f| f.subdivisions_per_beat == 4)
        .min_by(|a, b| (a.bpm - 173.4).abs().total_cmp(&(b.bpm - 173.4).abs()))
        .unwrap();
    assert!((f.bpm - 173.4).abs() < 0.1);
    assert!(f.p95_grid_residual_ms < 9.0);
    assert!(chart_fit(&ts, 75.0, 4).unwrap().coherence < 0.2);
    let a = chart_fit(&ts, 173.4, 4).unwrap();
    let b = chart_fit(&ts, 86.7, 8).unwrap();
    assert!((a.coherence - b.coherence).abs() < 1e-12);
}
#[test]
fn missing_audio_beats_do_not_change_refined_tempo() {
    let e = AudioEstimate {
        bpm: 154.7,
        beat_times_seconds: (0..200)
            .filter(|i| i % 7 != 3)
            .map(|i| 0.17 + i as f64 * 60.0 / 155.0)
            .collect(),
        tempo_prior_bpm: None,
    };
    let p = audio_pulse(&e).unwrap().unwrap();
    assert!((p.bpm - 155.0).abs() < 1e-8);
    assert!((p.beat_zero_seconds - 0.17).abs() < 1e-8);
    let mut short = e;
    short.beat_times_seconds.truncate(7);
    assert!(audio_pulse(&short).unwrap().is_none());
}
#[test]
fn authored_range_guides_alias_selection() {
    let ts = times(240.0, 0.1, 2, 300);
    let mut t = track(120.0);
    t.audio_estimates.extend(track(240.0).audio_estimates);
    assert!((select_grid(&ts, &t).unwrap().0.bpm - 120.0).abs() < 1e-8);
    t.authored_bpm_range = Some(AuthoredRange {
        minimum_bpm: 168.0,
        maximum_bpm: 325.0,
    });
    assert!((select_grid(&ts, &t).unwrap().0.bpm - 240.0).abs() < 1e-8);
}
#[test]
fn independent_phases_and_small_drift_retain_rhythmic_heads() {
    let charts = BTreeMap::from([
        ("a".into(), times(168.45, 0.137, 4, 300)),
        ("b".into(), times(168.6, 0.172, 4, 300)),
    ]);
    let ts = charts.values().flatten().copied().collect::<Vec<_>>();
    let t = track(168.5);
    let (g, p) = select_grid(&ts, &t).unwrap();
    let (grids, e) = select_shared(&charts, &t, &p, &g, &mut || Ok(())).unwrap();
    assert_eq!(e["summary"]["status"], "independent-chart-consensus");
    assert!((e["summary"]["consensusBpm"].as_f64().unwrap() - 168.525).abs() < 0.04);
    assert!((grids["a"].beat_zero_seconds - grids["b"].beat_zero_seconds).abs() > 0.01);
    for (id, ts) in charts {
        let c = chart(&id, ts);
        let (_, ep) = timing::quantize(&c, Some(&grids[&id])).unwrap();
        assert!(
            ep.iter()
                .filter(|e| matches!(e.method, QuantizationMethod::RhythmicSubdivision))
                .count()
                > ep.len() * 95 / 100
        );
        assert!(ep.iter().all(|e| e.shift_ms.abs() <= 9.0));
    }
}
#[test]
fn tempo_changes_and_phase_jumps_are_not_silently_discarded() {
    let mut ts = Vec::new();
    let mut windows = Vec::new();
    for (i, bpm) in [160.0, 160.0, 160.0, 175.0].iter().enumerate() {
        let period = 60.0 / bpm / 2.0;
        let first = ((i as f64 * 20.0 - 0.1) / period).ceil().max(0.0);
        let v = (0..40)
            .map(|j| 0.1 + (first + j as f64) * period)
            .collect::<Vec<_>>();
        ts.extend(&v);
        windows.push(ChartWindow {
            start_seconds: i as f64 * 20.0,
            end_seconds: i as f64 * 20.0 + 20.0,
            notes: v.len(),
            fit: chart_fit(&v, *bpm, 2).unwrap(),
        });
    }
    let e = dominant_grid(&ts, &windows, 0.2, &mut || Ok(())).unwrap();
    assert_eq!(e["inlierWindowIndices"], json!([0, 1, 2]));
    assert_eq!(e["excludedWindowIndices"], json!([3]));
    assert_eq!(e["retainedNotes"], 120);
    assert_eq!(e["automaticAcceptance"], false);
    assert!((e["fit"]["bpm"].as_f64().unwrap() - 160.0).abs() < 0.01);
}
#[test]
fn sparse_invalid_and_cancelled_evidence_reject() {
    assert!(chart_candidates(&[1.0, 2.0, 3.0], &mut || Ok(())).is_err());
    assert!(chart_fit(&[f64::NAN], 120.0, 2).is_err());
    let ts = times(120.0, 0.1, 2, 50);
    let e = chart_candidates(&ts, &mut || {
        Err(ddi_danoni_export::Error {
            code: "cancelled",
            message: "test".into(),
        })
    })
    .unwrap_err();
    assert_eq!(e.code, "cancelled");
    let t = TrackAnalysis {
        audio_estimates: vec![],
        ..track(120.0)
    };
    assert_eq!(select_grid(&ts, &t).unwrap_err().code, "insufficient-tempo");
}
#[test]
fn native_onsets_find_known_clicks_without_stereo_cancellation() {
    let rate = 22050;
    let bpm = 173.4;
    let mut samples = vec![0i16; rate as usize * 20 * 2];
    for i in 0..54 {
        let at = ((0.137 + i as f64 * 60.0 / bpm) * rate as f64).round() as usize;
        for j in 0..200 {
            let amp = (12000.0 * (1.0 - j as f64 / 200.0) * (j as f64 * 1.7).cos()) as i16;
            if (at + j) * 2 + 1 < samples.len() {
                samples[(at + j) * 2] = amp;
                samples[(at + j) * 2 + 1] = -amp;
            }
        }
    }
    let pcm = Pcm16 {
        rate,
        channels: 2,
        samples,
    };
    let report = onset_estimates(&pcm, &mut || Ok(())).unwrap();
    let pulses = report
        .estimates
        .iter()
        .filter_map(|e| audio_pulse(e).unwrap())
        .collect::<Vec<_>>();
    assert!(!pulses.is_empty());
    assert!(
        pulses.iter().any(|p| (p.bpm - bpm).abs() < 0.5),
        "{:?}",
        pulses
    );
    assert!(report.envelope.iter().any(|v| *v > 0.0));
    let mut silent = pcm;
    silent.samples.fill(0);
    assert!(
        onset_estimates(&silent, &mut || Ok(()))
            .unwrap()
            .estimates
            .is_empty()
    );
}
#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl ddi_danoni_export::assets::AssetSource for Memory {
    fn read(&self, _: &str, p: &str, m: usize) -> ddi_danoni_export::Result<Vec<u8>> {
        let b = &self.0[p];
        assert!(b.len() <= m);
        Ok(b.clone())
    }
}
fn bridge_fixture() -> (Memory, TempoRequest) {
    use ddi_danoni_export::assets::*;
    let mut source = Memory::default();
    let c = chart("synthetic", times(120.0, 0.1, 2, 100));
    let wav = Pcm16 {
        rate: 8000,
        channels: 1,
        samples: vec![0; 80000],
    }
    .wav()
    .unwrap();
    let audio_hash = sha256(&wav);
    source.0.insert("music.wav".into(), wav);
    let doc=serde_json::to_vec(&json!({"schema":"ddi-normalized-work/experimental-1","music":{"path":"music.wav","sha256":audio_hash},"charts":[c]})).unwrap();
    let hash = sha256(&doc);
    source.0.insert("source.json".into(), doc);
    let analysis=serde_json::to_vec(&json!({"schema":"ddi-tempo-analysis/experimental-1","sourceManifestSha256":hash,"tracks":[{"audioSha256":audio_hash,"chartIds":["synthetic"],"chartCandidates":[],"audioEstimates":track(120.0).audio_estimates}]})).unwrap();
    let analysis_hash = sha256(&analysis);
    source.0.insert("analysis.json".into(), analysis);
    let bind = |p: &str, h: String| Binding {
        namespace: "test".into(),
        path: p.into(),
        sha256: h,
    };
    let request = TempoRequest {
        manifest: bind("source.json", hash),
        media: vec![],
        legacy_analysis: Some(bind("analysis.json", analysis_hash)),
        cached: None,
    };
    (source, request)
}
#[test]
fn source_bound_decisions_cache_and_recheck_relationships() {
    use ddi_danoni_export::assets::*;
    let (mut source, mut request) = bridge_fixture();
    let fresh = prepare_tempo(&request, &source, &mut || Ok(())).unwrap();
    assert_eq!(fresh["status"], "estimated");
    let bytes = serde_json::to_vec(&fresh).unwrap();
    request.cached = Some(Binding {
        namespace: "test".into(),
        path: "cache.json".into(),
        sha256: sha256(&bytes),
    });
    source.0.insert("cache.json".into(), bytes);
    assert_eq!(
        prepare_tempo(&request, &source, &mut || Ok(())).unwrap(),
        fresh
    );
    let mut wrong = fresh.clone();
    wrong["cacheIdentity"]["policy"] = json!("changed");
    let b = serde_json::to_vec(&wrong).unwrap();
    request.cached.as_mut().unwrap().sha256 = sha256(&b);
    source.0.insert("cache.json".into(), b);
    assert!(prepare_tempo(&request, &source, &mut || Ok(())).is_err());
    request.cached = None;
    let mut doc: serde_json::Value = serde_json::from_slice(&source.0["source.json"]).unwrap();
    let v = doc["charts"][0]["lanes"][0]["tapsSeconds"][0].clone();
    doc["charts"][0]["lanes"][0]["tapsSeconds"]
        .as_array_mut()
        .unwrap()
        .push(v);
    let b = serde_json::to_vec(&doc).unwrap();
    request.manifest.sha256 = sha256(&b);
    source.0.insert("source.json".into(), b);
    assert_eq!(
        prepare_tempo(&request, &source, &mut || Ok(()))
            .unwrap_err()
            .code,
        "invalid-source-lane"
    );
}
#[test]
fn missing_pulses_cache_an_explicit_whole_work_fallback() {
    use ddi_danoni_export::assets::*;
    let (mut source, mut r) = bridge_fixture();
    let mut doc: serde_json::Value = serde_json::from_slice(&source.0["analysis.json"]).unwrap();
    doc["tracks"][0]["audioEstimates"] = json!([]);
    let b = serde_json::to_vec(&doc).unwrap();
    r.legacy_analysis.as_mut().unwrap().sha256 = sha256(&b);
    source.0.insert("analysis.json".into(), b);
    let result = prepare_tempo(&r, &source, &mut || Ok(())).unwrap();
    assert_eq!(result["status"], "exact-source-fallback");
    assert!(result["review"].is_null());
    let b = serde_json::to_vec(&result).unwrap();
    r.cached = Some(Binding {
        namespace: "test".into(),
        path: "cache.json".into(),
        sha256: sha256(&b),
    });
    source.0.insert("cache.json".into(), b);
    assert_eq!(prepare_tempo(&r, &source, &mut || Ok(())).unwrap(), result);
}

#[test]
fn native_media_receipt_and_algorithm_versions_bind_cache_reuse() {
    use ddi_danoni_export::assets::*;
    let (mut source, mut request) = bridge_fixture();
    request.legacy_analysis = None;
    let pcm = Binding {
        namespace: "test".into(),
        path: "music.wav".into(),
        sha256: sha256(&source.0["music.wav"]),
    };
    let proof = serde_json::to_vec(&json!({"schema":"ddi-danoni-prepared-media/1",
        "sourceManifestSha256":request.manifest.sha256,"sourceAudioSha256":pcm.sha256,
        "pcmSha256":pcm.sha256,"sampleRate":8000,"channels":1,"sampleFrames":80000}))
    .unwrap();
    let receipt = Binding {
        namespace: "test".into(),
        path: "receipt.json".into(),
        sha256: sha256(&proof),
    };
    source.0.insert("receipt.json".into(), proof);
    request.media = vec![AnalysisPcm {
        source_audio_sha256: pcm.sha256.clone(),
        pcm,
        receipt,
    }];
    let result = prepare_tempo(&request, &source, &mut || Ok(())).unwrap();
    assert_eq!(result["status"], "exact-source-fallback");
    assert_eq!(result["cacheIdentity"]["policy"], "ddi-tempo-selection/2");
    assert_eq!(
        result["cacheIdentity"]["quarterPhase"],
        "ddi-quarter-onset-support/1"
    );
    let bytes = serde_json::to_vec(&result).unwrap();
    request.cached = Some(Binding {
        namespace: "test".into(),
        path: "cache.json".into(),
        sha256: sha256(&bytes),
    });
    source.0.insert("cache.json".into(), bytes);
    assert_eq!(
        prepare_tempo(&request, &source, &mut || Ok(())).unwrap(),
        result
    );
    let mut old = result;
    old["cacheIdentity"]["onsets"] = json!("ddi-spectral-flux-dp/2");
    let bytes = serde_json::to_vec(&old).unwrap();
    request.cached.as_mut().unwrap().sha256 = sha256(&bytes);
    source.0.insert("cache.json".into(), bytes);
    assert!(prepare_tempo(&request, &source, &mut || Ok(())).is_err());
    request.cached = None;
    let mut proof: serde_json::Value = serde_json::from_slice(&source.0["receipt.json"]).unwrap();
    proof["sampleFrames"] = json!(79999);
    let bytes = serde_json::to_vec(&proof).unwrap();
    request.media[0].receipt.sha256 = sha256(&bytes);
    source.0.insert("receipt.json".into(), bytes);
    assert!(prepare_tempo(&request, &source, &mut || Ok(())).is_err());
}
