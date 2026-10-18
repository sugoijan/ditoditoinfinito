#![cfg(feature = "native")]
//! Local-only automatic-selector parity against the independent frozen corpus.
use ddi_danoni_export::{
    assets::*,
    audio::{self, DecodeOptions, MediaRequest},
    extraction::*,
    native::DirectorySource,
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
// Worker iterations/output sample cursors are replay provenance. Native
// logical origin, stream slice and all other music metadata remain exact.
fn compare_origin(actual: &Value, old: &Value, path: &str) -> Value {
    let step = actual["iteration"].as_i64().unwrap() - old["iteration"].as_i64().unwrap();
    // The reviewed worker mixes 44.1 kHz PCM at the profiles' verified 60 Hz.
    // samplesPerMovieFrame belongs to the source stream head, not this mixer.
    let samples = 44100. / 60.;
    let output =
        actual["outputSampleFrame"].as_f64().unwrap() - old["outputSampleFrame"].as_f64().unwrap();
    assert!(
        (output - step as f64 * samples).abs() < 1e-8,
        "{path}: replay step/sample displacement disagrees"
    );
    let mut expected = old.clone();
    expected["iteration"] = actual["iteration"].clone();
    expected["outputSampleFrame"] = actual["outputSampleFrame"].clone();
    compare(actual, &expected, path);
    json!({"workerIterationDifference":step,"outputSampleFrameDifference":output,"nativeLogicalFrameUnchanged":true,"otherStreamFieldsUnchanged":true})
}
#[test]
fn replay_epoch_changes_must_explain_sample_cursor_without_moving_native_time() {
    let old = json!({"iteration":201,"outputSampleFrame":147000,"samplesPerMovieFrame":735,"logicalFrame":145,"sliceStart":100,"sliceEnd":200,"clock":"frame_num"});
    let mut replay = old.clone();
    replay["iteration"] = json!(203);
    replay["outputSampleFrame"] = json!(148470);
    assert_eq!(
        compare_origin(&replay, &old, "origin")["workerIterationDifference"],
        2
    );
    for field in [
        "outputSampleFrame",
        "logicalFrame",
        "sliceStart",
        "samplesPerMovieFrame",
    ] {
        let mut bad = replay.clone();
        bad[field] = json!(bad[field].as_i64().unwrap() + 1);
        assert!(
            std::panic::catch_unwind(|| compare_origin(&bad, &old, "bad")).is_err(),
            "{field}"
        );
    }
    replay["iteration"] = json!(199);
    replay["outputSampleFrame"] = json!(145530);
    compare_origin(&replay, &old, "earlier replay");
    let mut half_rate = old.clone();
    half_rate["samplesPerMovieFrame"] = json!(367);
    half_rate["sampleRate"] = json!(22050);
    let mut replay = half_rate.clone();
    replay["iteration"] = json!(203);
    replay["outputSampleFrame"] = json!(148470);
    compare_origin(&replay, &half_rate, "source head is not output stride");
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
fn automatic_profiles_match_frozen_charts_and_feed_the_game_parser() {
    let Some(path) = std::env::var_os("DDI_DANONI_PROFILE_CASES") else {
        eprintln!("SKIP: set DDI_DANONI_PROFILE_CASES");
        return;
    };
    let table = read(Path::new(&path));
    assert_eq!(table["schema"], "ddi-danoni-profile-parity-cases/1");
    let root = PathBuf::from(table["root"].as_str().unwrap());
    let mut checks = vec![];
    let rejected_source =
        DirectorySource::new(BTreeMap::from([("corpus".into(), root.clone())])).unwrap();
    for row in table["rejections"].as_array().unwrap() {
        let binding: Binding = serde_json::from_value(row["failure"].clone()).unwrap();
        let b = rejected_source
            .read(&binding.namespace, &binding.path, JSON_LIMIT)
            .unwrap();
        assert_eq!(sha256(&b), binding.sha256);
        let failure: Value = serde_json::from_slice(&b).unwrap();
        assert_eq!(failure["ready"], false);
        assert_eq!(failure["failure"]["code"], row["code"]);
        assert!(failure["normalized"].is_null());
        assert!(
            !root
                .join(&binding.path)
                .parent()
                .unwrap()
                .join("manifest.json")
                .exists()
        );
        assert!(!failure["motionTrials"].as_array().unwrap().is_empty());
        assert!(
            failure["motionTrials"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["passed"] == false)
        );
    }
    let (mut charts, mut taps, mut holds, mut endpoints, mut max_error, mut max_pcm_difference) =
        (0, 0, 0, 0, 0f64, 0i32);
    for row in table["cases"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let profile = root.join(row["profileOutput"].as_str().unwrap());
        let source = DirectorySource::new(BTreeMap::from([
            ("corpus".into(), root.clone()),
            ("profile".into(), profile.clone()),
        ]))
        .unwrap();
        let parent = read(&profile.join("manifest.json"));
        assert_eq!(parent["ready"], true);
        let request: NativeWorkRequest = serde_json::from_value(row["request"].clone()).unwrap();
        assert_eq!(
            request.charts.len(),
            parent["normalized"]["charts"].as_u64().unwrap() as usize
        );
        let work = prepare_native_work(&request, &source, &mut || Ok(()))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let baseline_binding: Binding = serde_json::from_value(row["baseline"].clone()).unwrap();
        let b = source
            .read(
                &baseline_binding.namespace,
                &baseline_binding.path,
                JSON_LIMIT,
            )
            .unwrap();
        assert_eq!(sha256(&b), baseline_binding.sha256);
        let baseline: Value = serde_json::from_slice(&b).unwrap();
        let prefix = Path::new(&baseline_binding.path).parent().unwrap();
        let mut generated = Memory(
            work.files
                .iter()
                .map(|f| (f.path.clone(), f.bytes.clone()))
                .collect(),
        );
        let mut differences = vec![];
        for c in work.manifest["charts"].as_array().unwrap() {
            let name = c["id"].as_str().unwrap();
            let old_id = row["chartMapping"][name].as_str().unwrap();
            let old = baseline["charts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == old_id)
                .unwrap();
            // Every gameplay/source-data field is compared. Only paths and
            // independently reproved capture iterations are presentation below.
            let provenance = [
                "id",
                "nativeReference",
                "music",
                "effects",
                "preparedIteration",
                "splitIteration",
                "compiledIteration",
                "clockInitializationIteration",
                "audioOrigin",
            ];
            for (key, value) in old.as_object().unwrap() {
                if !provenance.contains(&key.as_str()) {
                    compare(&c[key], value, &format!("{id}.{name}.{key}"));
                }
            }
            for key in c.as_object().unwrap().keys() {
                assert!(
                    old.get(key).is_some() || key == "compiledIteration",
                    "unexpected new chart field {key}"
                );
            }
            let origin_difference = compare_origin(
                &c["audioOrigin"],
                &old["audioOrigin"],
                &format!("{id}.{name}.audioOrigin"),
            );
            for reference in [c, old] {
                if let Some(clock) = reference.get("clockInitializationIteration") {
                    assert_eq!(
                        clock, &reference["preparedIteration"],
                        "Clock-init provenance must be the independently proved zero-counter witness"
                    );
                }
            }
            let mut actual_effects = c["effects"].clone();
            let mut expected_effects = old["effects"].clone();
            for effects in [&mut actual_effects, &mut expected_effects] {
                for e in effects.as_array_mut().unwrap() {
                    if e["type"] == "missing-optional-color" {
                        for r in e["review"]["missingResources"].as_array_mut().unwrap() {
                            r.as_object_mut().unwrap().remove("path");
                        }
                        for d in e["diagnostics"].as_array_mut().unwrap() {
                            d.as_object_mut().unwrap().remove("request");
                        }
                    }
                }
            }
            compare(
                &actual_effects,
                &expected_effects,
                &format!("{id}.{name}.retainedEffects"),
            );
            let music = c["music"]["path"].as_str().unwrap();
            let actual = audio::decode(&generated.0[music], &DecodeOptions::default(), &mut || {
                Ok(())
            })
            .unwrap();
            let old_audio = fs::read(
                root.join(prefix)
                    .join(old["music"]["path"].as_str().unwrap()),
            )
            .unwrap();
            assert_eq!(sha256(&old_audio), old["music"]["sha256"]);
            let wanted =
                audio::decode(&old_audio, &DecodeOptions::default(), &mut || Ok(())).unwrap();
            assert_eq!(
                (actual.pcm.rate, actual.pcm.channels, actual.pcm.frames()),
                (wanted.pcm.rate, wanted.pcm.channels, wanted.pcm.frames())
            );
            let delta = actual
                .pcm
                .samples
                .iter()
                .zip(&wanted.pcm.samples)
                .map(|(a, b)| (*a as i32 - *b as i32).abs())
                .max()
                .unwrap();
            assert!(delta <= 1, "{id}.{name}: PCM difference {delta}");
            max_pcm_difference = max_pcm_difference.max(delta);
            differences.push(json!({"chart":name,"frozenChart":old_id,"canonicalPcmMaxDifference":delta,"previousPreparedIteration":old["preparedIteration"],"currentPreparedIteration":c["preparedIteration"],"previousSplitIteration":old["splitIteration"],"currentSplitIteration":c["splitIteration"],"compiledIteration":c["compiledIteration"],"audioOriginReplay":origin_difference,"reviewedProvenanceChanges":["selector IDs/reference paths","capture iterations and source-proved zero-counter witnesses","worker-origin iterations/output cursors displaced together by exact native sample stride","WAV encoder bytes within one PCM unit","relocated optional-colour evidence paths","structured GET evidence on reviewed optional-colour denial"]}));
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

        checks.push(json!({"work":id,"charts":parsed_charts,"captures":parent["captures"].as_object().unwrap().len(),"motionTrials":parent["motionTrials"],"differences":differences,"scrollChecks":work_scroll_checks}));
        eprintln!("profile {id}: {parsed_charts} charts");
    }
    assert_eq!(charts, table["expectedCharts"].as_u64().unwrap() as usize);
    assert_eq!(
        checks.len(),
        table["expectedWorks"].as_u64().unwrap() as usize
    );
    let receipt = json!({"schema":"ddi-danoni-profile-parity-result/1","casesSha256":sha256(&fs::read(&path).unwrap()),"works":checks.len(),"rejections":table["rejections"].as_array().unwrap().len(),"charts":charts,"taps":taps,"holds":holds,"endpoints":endpoints,"maximumTimingErrorMs":max_error,"canonicalPcmMaxDifference":max_pcm_difference,"checks":checks});
    if let Some(out) = std::env::var_os("DDI_DANONI_PROFILE_RECEIPT") {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out)
            .unwrap();
        std::io::Write::write_all(
            &mut f,
            serde_json::to_vec_pretty(&receipt).unwrap().as_slice(),
        )
        .unwrap();
    }
    eprintln!(
        "{charts} charts / {taps} taps / {holds} holds / {endpoints} endpoints; {max_error} ms; PCM {max_pcm_difference}"
    );
}
