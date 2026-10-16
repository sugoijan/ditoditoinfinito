#![cfg(feature = "native")]
//! Local-only complete normalized producer gate and in-memory downstream smoke.
use ddi_danoni_export::{
    assets::*,
    audio::{self, DecodeOptions, MediaRequest},
    extraction::*,
    native::{self, DirectorySource},
    work::*,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
fn read(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
fn compare(a: &Value, b: &Value, path: &str) {
    if a.is_number() && b.is_number() {
        let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
        assert!(
            (a - b).abs() <= 1e-8 + 1e-12 * a.abs().max(b.abs()),
            "{path}: {a} != {b}"
        );
        return;
    }
    match (a, b) {
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path}"
            );
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
struct Memory(BTreeMap<String, Arc<Vec<u8>>>);
impl AssetSource for Memory {
    fn read(&self, namespace: &str, path: &str, max: usize) -> ddi_danoni_export::Result<Vec<u8>> {
        assert_eq!(namespace, "generated");
        let b = &self.0[path];
        assert!(b.len() <= max);
        Ok(b.as_ref().clone())
    }
}
fn binding(m: &Memory, path: &str) -> Binding {
    Binding {
        namespace: "generated".into(),
        path: path.into(),
        sha256: sha256(&m.0[path]),
    }
}
#[test]
fn saved_native_works_normalize_and_feed_the_existing_rust_song_pipeline() {
    let Some(path) = std::env::var_os("DDI_DANONI_NORMALIZATION_CASES") else {
        eprintln!("SKIP: set DDI_DANONI_NORMALIZATION_CASES");
        return;
    };
    let table = read(Path::new(&path));
    assert_eq!(table["schema"], "ddi-danoni-normalization-parity-cases/1");
    let root = PathBuf::from(table["root"].as_str().unwrap());
    let source = DirectorySource::new(BTreeMap::from([("corpus".into(), root.clone())])).unwrap();
    let out = std::env::var_os("DDI_DANONI_NORMALIZATION_OUTPUT").map(PathBuf::from);
    if let Some(out) = &out {
        fs::create_dir(out).expect("fresh external normalization corpus");
    }
    let (mut charts, mut taps, mut holds, mut endpoints, mut max_error, mut max_pcm_difference) =
        (0, 0, 0, 0, 0f64, 0i32);
    let mut checks = vec![];
    let mut failures = vec![];
    for row in table["rejections"].as_array().unwrap() {
        let request: NativeWorkRequest = serde_json::from_value(row["request"].clone()).unwrap();
        let e = prepare_native_work(&request, &source, &mut || Ok(()))
            .err()
            .expect("strict proof rejection");
        assert_eq!(e.code, row["code"].as_str().unwrap(), "{}: {e}", row["id"]);
        failures.push(json!({"id":row["id"],"error":e}));
    }
    for row in table["cases"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let request: NativeWorkRequest = serde_json::from_value(row["request"].clone()).unwrap();
        let work = prepare_native_work(&request, &source, &mut || Ok(()))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let pinned = row.get("pinnedReferences").is_some();
        let baseline_binding: Option<Binding> = row
            .get("baseline")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        let baseline = baseline_binding.as_ref().map(|b| {
            let bytes = source.read("corpus", &b.path, JSON_LIMIT).unwrap();
            assert_eq!(sha256(&bytes), b.sha256);
            serde_json::from_slice::<Value>(&bytes).unwrap()
        });
        let prefix = baseline_binding
            .as_ref()
            .map(|b| Path::new(&b.path).parent().unwrap());
        let mut generated = Memory(
            work.files
                .iter()
                .map(|f| (f.path.clone(), f.bytes.clone()))
                .collect(),
        );
        let mut differences = vec![];
        let mut compared_pcm = BTreeMap::new();
        for c in work.manifest["charts"].as_array().unwrap() {
            let name = c["id"].as_str().unwrap();
            if pinned {
                let witness = row["pinnedReferences"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["id"] == name)
                    .unwrap();
                let read_bound = |key: &str| {
                    let b: Binding = serde_json::from_value(witness[key].clone()).unwrap();
                    let bytes = source.read(&b.namespace, &b.path, MEDIA_LIMIT).unwrap();
                    assert_eq!(sha256(&bytes), b.sha256);
                    bytes
                };
                let expected: Value = serde_json::from_slice(&read_bound("binding")).unwrap();
                let mut comparable_pinned = c["pinnedReference"].clone();
                let mut reviewed_worker_change = Value::Null;
                if witness.get("currentWorkerBuild").is_some() {
                    let previous: Value =
                        serde_json::from_slice(&read_bound("previousWorkerBuild")).unwrap();
                    let current: Value =
                        serde_json::from_slice(&read_bound("currentWorkerBuild")).unwrap();
                    assert_eq!(expected["ruffle"], previous);
                    assert_eq!(comparable_pinned["ruffle"], current);
                    assert_eq!(
                        current,
                        ddi_danoni_export::supervisor::expected_build_identity()
                    );
                    // Source changes are explicit, hash-bound provenance. Current
                    // identity must still equal this SDK's reviewed worker; no
                    // gameplay/reference field is exempted by this comparison.
                    let fields = witness.get("workerSourceChanges").map_or_else(
                        || vec!["src/navigator.rs"],
                        |v| {
                            v.as_array()
                                .unwrap()
                                .iter()
                                .map(|p| p.as_str().unwrap())
                                .collect()
                        },
                    );
                    assert!(!fields.is_empty());
                    assert_eq!(fields.iter().collect::<BTreeSet<_>>().len(), fields.len());
                    let mut changes = Vec::new();
                    let mut before = previous.clone();
                    let mut after = current.clone();
                    for field in fields {
                        let old_hash = before["files"].as_object_mut().unwrap().remove(field);
                        let new_hash = after["files"].as_object_mut().unwrap().remove(field);
                        assert_ne!(
                            old_hash, new_hash,
                            "unchanged declared worker source {field}"
                        );
                        changes.push(json!({"field":field,"previous":old_hash,"current":new_hash}));
                    }
                    compare(&before, &after, "unchanged worker sources/revision");
                    reviewed_worker_change = json!({"files":changes,"previousBuild":witness["previousWorkerBuild"],"currentBuild":witness["currentWorkerBuild"]});
                    comparable_pinned["ruffle"] = previous;
                }
                compare(
                    &comparable_pinned,
                    &expected,
                    &format!("{id}.{name}.pinnedReference"),
                );
                let frozen: Value = serde_json::from_slice(&read_bound("frozenReference")).unwrap();
                for key in ["lanes", "speed", "colors", "boost"] {
                    compare(
                        &expected[key],
                        &frozen[key],
                        &format!("{id}.{name}.frozen.{key}"),
                    );
                }
                for (lane, old) in c["lanes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(expected["lanes"].as_array().unwrap())
                {
                    for key in ["tapsSeconds", "holdsSeconds"] {
                        compare(&lane[key], &old[key], &format!("{id}.{name}.{key}"));
                    }
                }
                compare(
                    &c["difficultyStars"],
                    &expected["metadata"]["difficultyStars"],
                    "numeric difficulty",
                );
                let music = c["music"]["path"].as_str().unwrap();
                let actual =
                    audio::decode(&generated.0[music], &DecodeOptions::default(), &mut || {
                        Ok(())
                    })
                    .unwrap();
                let wanted = audio::decode(
                    &read_bound("decodedMusic"),
                    &DecodeOptions::default(),
                    &mut || Ok(()),
                )
                .unwrap();
                assert_eq!(
                    (actual.pcm.rate, actual.pcm.channels, actual.pcm.frames()),
                    (wanted.pcm.rate, wanted.pcm.channels, wanted.pcm.frames())
                );
                let difference = actual
                    .pcm
                    .samples
                    .iter()
                    .zip(&wanted.pcm.samples)
                    .map(|(a, b)| (*a as i32 - *b as i32).abs())
                    .max()
                    .unwrap();
                assert!(difference <= 1, "{id} {name}: PCM difference {difference}");
                max_pcm_difference = max_pcm_difference.max(difference);
                let origin: Value = serde_json::from_slice(
                    &generated.0[&format!("references/{name}-audio-origin.json")],
                )
                .unwrap();
                assert_eq!(
                    origin["appliedShiftSamples"],
                    witness["expectedShiftSamples"]
                );
                let motion: Value =
                    serde_json::from_slice(&generated.0[&format!("references/{name}-motion.json")])
                        .unwrap();
                compare(
                    &motion["baseArrowHeightsPerSecond"],
                    &witness["expectedBaseArrowHeightsPerSecond"],
                    "pinned reference width",
                );
                assert_eq!(motion["arrivalProved"], false);
                differences.push(json!({"chart":name,"pinnedReferenceParity":true,"reviewedWorkerIdentityChange":reviewed_worker_change,"frozenNoteEffectParity":true,"canonicalPcmMaxDifference":difference,"appliedShiftSamples":origin["appliedShiftSamples"]}));
            } else {
                let baseline = baseline.as_ref().unwrap();
                let prefix = prefix.unwrap();
                let old = baseline["charts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v["id"] == name)
                    .unwrap();
                for (k, v) in old.as_object().unwrap() {
                    if k != "music" {
                        compare(&c[k], v, &format!("{id}.{name}.{k}"));
                    }
                }
                let added = c
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|k| old.get(*k).is_none())
                    .cloned()
                    .collect::<Vec<_>>();
                assert!(
                    added.iter().all(|k| [
                        "initialPixelSpeed",
                        "compiledIteration",
                        "clockInitializationIteration",
                        "inactiveSpeedTail",
                        "inactiveSourceSpeed"
                    ]
                    .contains(&k.as_str())),
                    "{id} {name}: {added:?}"
                );
                let music = c["music"]["path"].as_str().unwrap();
                let pcm_diff = if let Some(d) = compared_pcm.get(music) {
                    *d
                } else {
                    let actual =
                        audio::decode(&generated.0[music], &DecodeOptions::default(), &mut || {
                            Ok(())
                        })
                        .unwrap();
                    let old_bytes = fs::read(
                        root.join(prefix)
                            .join(old["music"]["path"].as_str().unwrap()),
                    )
                    .unwrap();
                    assert_eq!(sha256(&old_bytes), old["music"]["sha256"]);
                    let wanted =
                        audio::decode(&old_bytes, &DecodeOptions::default(), &mut || Ok(()))
                            .unwrap();
                    assert_eq!(
                        (actual.pcm.rate, actual.pcm.channels, actual.pcm.frames()),
                        (wanted.pcm.rate, wanted.pcm.channels, wanted.pcm.frames()),
                        "{id} {name}"
                    );
                    let difference = actual
                        .pcm
                        .samples
                        .iter()
                        .zip(&wanted.pcm.samples)
                        .map(|(a, b)| (*a as i32 - *b as i32).abs())
                        .max()
                        .unwrap();
                    assert!(difference <= 1, "{id} {name}: PCM difference {difference}");
                    compared_pcm.insert(music.to_string(), difference);
                    difference
                };
                max_pcm_difference = max_pcm_difference.max(pcm_diff);
                let origin: Value = serde_json::from_slice(
                    &generated.0[&format!("references/{name}-audio-origin.json")],
                )
                .unwrap();
                let mp3 = c["music"]["path"]
                    .as_str()
                    .unwrap()
                    .rsplit_once('/')
                    .unwrap()
                    .0;
                let old_origin = read(&root.join(prefix).join(mp3).join("audio-origin.json"));
                assert_eq!(
                    origin["appliedShiftSamples"], old_origin["appliedShiftSamples"],
                    "{id} {name}"
                );
                differences.push(json!({"chart":name,"newFields":added,"canonicalPcmMaxDifference":pcm_diff,"appliedShiftSamples":origin["appliedShiftSamples"]}));
            }
            charts += 1;
            for lane in c["lanes"].as_array().unwrap() {
                taps += lane["tapsSeconds"].as_array().unwrap().len();
                holds += lane["holdsSeconds"].as_array().unwrap().len();
            }
        }
        let manifest_binding = binding(&generated, "manifest.json");
        let motion_binding = binding(&generated, "native-scroll-review.json");
        let mut supplied = vec![];
        let music = work.manifest["charts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["music"]["sha256"].as_str().unwrap().to_string(),
                    c["music"]["path"].as_str().unwrap().to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let expected_songs = music.len();
        for (hash, path) in music {
            let request = MediaRequest {
                manifest: manifest_binding.clone(),
                audio: binding(&generated, &path),
                codec: Codec::Wav,
                decode: DecodeOptions::default(),
                quality: 0.5,
            };
            let media = audio::prepare_media(&request, &generated, &mut || Ok(())).unwrap();
            let prefix = format!("codec/{hash}");
            for f in media.files {
                generated.0.insert(format!("{prefix}/{}", f.path), f.bytes);
            }
            let receipt = format!("{prefix}/manifest.json");
            generated.0.insert(
                receipt.clone(),
                Arc::new(serde_json::to_vec_pretty(&media.manifest).unwrap()),
            );
            supplied.push(SuppliedMedia {
                source_audio_sha256: hash,
                audio: binding(&generated, &format!("{prefix}/music.wav")),
                pcm: binding(&generated, &format!("{prefix}/pcm.wav")),
                codec: Codec::Wav,
                provenance: binding(&generated, &receipt),
            });
        }
        let prepared = prepare_work(
            &WorkRequest {
                manifest: manifest_binding,
                reviews: ReviewBindings {
                    motion: Some(motion_binding),
                    ..Default::default()
                },
                media: supplied,
                options: WorkOptions::default(),
            },
            &generated,
            &mut |_| Ok(()),
        )
        .unwrap_or_else(|e| panic!("{id}: downstream {e}"));
        assert_eq!(
            prepared.manifest["songs"].as_array().unwrap().len(),
            expected_songs
        );
        let mut parsed_charts = 0;
        let mut work_scroll_checks = 0;
        for record in prepared.manifest["songs"].as_array().unwrap() {
            let file = prepared
                .files
                .iter()
                .find(|f| f.path == record["simfile"].as_str().unwrap())
                .unwrap();
            let song = ddi_chart::formats::sm::parse_ssc(std::str::from_utf8(&file.bytes).unwrap())
                .unwrap();
            let reports = record["charts"].as_array().unwrap();
            assert_eq!(reports.len(), song.charts.len());
            for (report, chart) in reports.iter().zip(&song.charts) {
                if pinned {
                    let source_chart = work.manifest["charts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|c| c["id"] == report["id"])
                        .unwrap();
                    let stars = source_chart["difficultyStars"].as_u64().map(|n| n as u16);
                    assert_eq!(report["difficultyStars"], source_chart["difficultyStars"]);
                    assert_eq!(chart.meter, stars.unwrap_or(0) as u32);
                    let tier =
                        ddi_danoni_export::policy::tier_for_stars(stars, [1, 3, 5, 7]).unwrap();
                    assert_eq!(format!("{:?}", chart.difficulty), tier.label());
                }
                let timing = chart.timing(&song);
                let padding = report["paddingTicks"].as_i64().unwrap();
                let mut actual = BTreeSet::new();
                for note in &chart.notes {
                    let kind = match note.kind {
                        ddi_chart::NoteKind::Tap => "tap",
                        ddi_chart::NoteKind::HoldHead { end } => {
                            actual.insert((note.lane as u64, "hold-tail".to_string(), end.0));
                            "hold-head"
                        }
                        _ => panic!("Unexpected exported note kind"),
                    };
                    assert!(actual.insert((note.lane as u64, kind.to_string(), note.tick.0)));
                }
                let expected = report["endpoints"].as_array().unwrap();
                assert_eq!(expected.len(), actual.len());
                let mut chart_error = 0f64;
                for endpoint in expected {
                    let tick = endpoint["tick"].as_i64().unwrap() + padding;
                    assert!(actual.remove(&(
                        endpoint["lane"].as_u64().unwrap(),
                        endpoint["kind"].as_str().unwrap().to_string(),
                        tick
                    )));
                    let parsed_seconds = timing.seconds_at(ddi_chart::Tick(tick));
                    let source_seconds = endpoint["sourceSeconds"].as_f64().unwrap();
                    assert!(
                        (parsed_seconds - endpoint["exportSeconds"].as_f64().unwrap()).abs() < 1e-8
                    );
                    let error = (parsed_seconds - source_seconds).abs() * 1000.0;
                    assert!(error <= 9.0 + 1e-7);
                    chart_error = chart_error.max(error);
                    endpoints += 1;
                }
                assert!(actual.is_empty());
                assert!(
                    (chart_error - report["maximumNoteErrorMs"].as_f64().unwrap()).abs() < 1e-6
                );
                max_error = max_error.max(chart_error);
                let base = report["nativeMotion"]["baseArrowHeightsPerSecond"]
                    .as_f64()
                    .unwrap();
                let mut segments = BTreeMap::from([(0i64, 1f64)]);
                for event in report["scrollEvents"].as_array().unwrap() {
                    segments.insert(
                        event["tick"].as_i64().unwrap() + padding,
                        event["multiplier"].as_f64().unwrap(),
                    );
                }
                let segments = segments.into_iter().collect::<Vec<_>>();
                for (i, (tick, multiplier)) in segments.iter().enumerate() {
                    let start = timing.seconds_at(ddi_chart::Tick(*tick));
                    let dt = segments.get(i + 1).map_or(0.01, |(next, _)| {
                        ((timing.seconds_at(ddi_chart::Tick(*next)) - start) / 4.0).min(0.01)
                    });
                    assert!(dt > 0.0);
                    for x in [1.0, 2.0, 3.0] {
                        let options = ddi_engine::scroll::ScrollOptions {
                            speed: ddi_engine::scroll::SpeedMod::XMod(x),
                            ..Default::default()
                        };
                        let target = ddi_chart::Tick(*tick + 1);
                        let first = ddi_engine::scroll::note_y(options, timing, target, start + dt);
                        let second =
                            ddi_engine::scroll::note_y(options, timing, target, start + 2.0 * dt);
                        let velocity = (first as f64 - second as f64) / dt;
                        let expected = base * multiplier * x;
                        assert!(
                            (velocity - expected).abs() <= 1e-4 + expected.abs() * 1e-4,
                            "{id}: speed {velocity} != {expected}"
                        );
                        work_scroll_checks += 1;
                    }
                }
                parsed_charts += 1;
            }
        }
        assert_eq!(
            parsed_charts,
            work.manifest["charts"].as_array().unwrap().len()
        );
        if let Some(out) = &out {
            native::write_normalized(&work, &out.join(id), &mut || Ok(())).unwrap();
        }
        checks.push(json!({"work":id,"charts":parsed_charts,"files":work.files.len(),"sourceInputs":work.receipt["sourceInputs"].as_object().unwrap().len(),"differences":differences,"decorationJoined":work.receipt["charts"].as_array().unwrap().iter().filter(|r|r["presentationDiagnostics"]["audio"].as_array().is_some_and(|v|!v.is_empty())).count(),"downstreamSongs":prepared.manifest["songs"].as_array().unwrap().len(),"scrollChecks":work_scroll_checks}));
        eprintln!("normalized-work {id}: {parsed_charts} charts");
    }
    assert_eq!(
        checks.len(),
        table["expectedWorks"].as_u64().unwrap() as usize
    );
    assert_eq!(charts, table["expectedCharts"].as_u64().unwrap() as usize);
    assert_eq!(
        failures.len(),
        table["expectedRejections"].as_u64().unwrap() as usize
    );
    assert!(max_error <= 9.0);
    assert_eq!(endpoints, taps + 2 * holds);
    let report = json!({"schema":"ddi-danoni-rust-normalization-parity/1","passed":true,"casesSha256":sha256(&fs::read(path).unwrap()),"checks":checks,"strictRejections":failures,"charts":charts,"taps":taps,"holds":holds,"endpoints":endpoints,"maximumErrorMs":max_error,"maximumCanonicalPcmDifference":max_pcm_difference,"scope":"Passive bound runs to normalized artifacts, independently aligned PCM and downstream WAV/SSC preparation; fallback source grid, no worker launched by Rust or BPM acceptance"});
    if let Some(out) = out {
        fs::write(
            out.join("manifest.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    eprintln!("normalized corpus: {} works, {charts} charts", checks.len());
}
