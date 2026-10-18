#![cfg(feature = "native")]
//! All research data stays local. Compatibility and fresh native estimates are
//! different gates and their provenance is deliberately separate.
use ddi_danoni_export::{assets::*, model::*, tempo::*, work::*};
use serde_json::{Value, json};
use std::{fs, path::Path};
fn read(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
struct Bundle<'a> {
    dir: &'a Path,
    w: &'a Value,
    legacy: Option<Vec<u8>>,
    media: Option<&'a Path>,
    cache: Option<Vec<u8>>,
}
impl AssetSource for Bundle<'_> {
    fn read(&self, ns: &str, path: &str, max: usize) -> ddi_danoni_export::Result<Vec<u8>> {
        let b = match ns {
            "legacy" => self.legacy.as_ref().unwrap().clone(),
            "cache" => self.cache.as_ref().unwrap().clone(),
            "media" => fs::read(self.media.unwrap().join(path)).unwrap(),
            _ => {
                let key = format!("{ns}:{path}");
                let p = self.w["inputArtifacts"][&key].as_str().unwrap();
                fs::read(self.dir.join(p)).unwrap()
            }
        };
        assert!(b.len() <= max);
        Ok(b)
    }
}
#[test]
fn saved_analysis_reproduces_accepted_grid_decisions() {
    let Some(root) = std::env::var_os("DDI_DANONI_TEMPO_GENERATION") else {
        eprintln!("SKIP: set DDI_DANONI_TEMPO_GENERATION");
        return;
    };
    let Some(exports) = std::env::var_os("DDI_DANONI_EXPORTS") else {
        eprintln!("SKIP: set DDI_DANONI_EXPORTS");
        return;
    };
    let (root, exports) = (Path::new(&root), Path::new(&exports));
    let generation = read(&root.join("manifest.json"));
    let mut checks = Vec::new();
    let mut failures = Vec::new();
    for row in generation["works"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let path = exports
            .join("reports")
            .join(id)
            .join("automatic-tempo/analysis.json");
        if !path.exists() {
            continue;
        }
        let raw = fs::read(path).unwrap();
        let dir = root.join("works").join(id);
        let w = read(&dir.join("manifest.json"));
        let work: WorkRequest = serde_json::from_value(w["request"].clone()).unwrap();
        let binding = Binding {
            namespace: "legacy".into(),
            path: "analysis.json".into(),
            sha256: sha256(&raw),
        };
        let source = Bundle {
            dir: &dir,
            w: &w,
            legacy: Some(raw),
            media: None,
            cache: None,
        };
        let request = TempoRequest {
            manifest: work.manifest.clone(),
            media: vec![],
            legacy_analysis: Some(binding),
            cached: None,
        };
        let result = prepare_tempo(&request, &source, &mut || Ok(()));
        match result {
            Ok(result) => {
                let old_grid_path = exports
                    .join("reports")
                    .join(id)
                    .join("musical-grid-review.json");
                if old_grid_path.exists() {
                    let old = read(&old_grid_path);
                    if result["status"] != "estimated" {
                        failures.push(json!({"work":id,"reason":result["fallback"]}));
                        continue;
                    }
                    for song in w["songs"].as_array().unwrap() {
                        for c in song["charts"].as_array().unwrap() {
                            let cid = c["id"].as_str().unwrap();
                            let a = &result["review"]["charts"][cid];
                            let b = &old["charts"][cid];
                            let a_bpm = a["bpm"].as_f64().unwrap();
                            let b_bpm = b["bpm"].as_f64().unwrap();
                            let period = 60.0 / b_bpm;
                            let phase_delta = ((a["beatZeroSeconds"].as_f64().unwrap()
                                - b["beatZeroSeconds"].as_f64().unwrap()
                                + period / 2.0)
                                .rem_euclid(period)
                                - period / 2.0)
                                .abs();
                            let mismatch = (a_bpm - b_bpm).abs() > 0.001
                                || phase_delta > 0.0001
                                || a["subdivisionsPerBeat"] != b["subdivisionsPerBeat"]
                                || a["estimationMethod"] != b["estimationMethod"];
                            if mismatch {
                                failures.push(json!({"work":id,"chart":cid,"new":a,"old":b,"phaseDeltaSeconds":phase_delta}));
                            } else {
                                checks.push(json!({"work":id,"chart":cid,"bpm":a_bpm,"phaseDeltaSeconds":phase_delta,"method":a["estimationMethod"]}));
                            }
                        }
                    }
                } else {
                    assert_eq!(result["status"], "exact-source-fallback");
                    checks.push(json!({"work":id,"fallback":true}));
                }
            }
            Err(e) => failures.push(json!({"work":id,"error":e})),
        }
    }
    println!(
        "{}",
        json!({"schema":"ddi-danoni-rust-tempo-bridge-parity/1","checks":checks,"failures":failures,"passed":failures.is_empty(),"scope":"Rust decision policy over preserved analysis; native audio onsets are not supplied by this gate"})
    );
    assert!(!checks.is_empty());
    assert!(failures.is_empty(), "{} policy differences", failures.len());
}
#[test]
fn recomputed_lattices_match_saved_statistics() {
    let Some(root) = std::env::var_os("DDI_DANONI_TEMPO_GENERATION") else {
        eprintln!("SKIP: set DDI_DANONI_TEMPO_GENERATION");
        return;
    };
    let Some(exports) = std::env::var_os("DDI_DANONI_EXPORTS") else {
        return;
    };
    let (root, exports) = (Path::new(&root), Path::new(&exports));
    let g = read(&root.join("manifest.json"));
    let mut checks = Vec::new();
    let mut failures = Vec::new();
    for row in g["works"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let p = exports
            .join("reports")
            .join(id)
            .join("automatic-tempo/analysis.json");
        if !p.exists() {
            continue;
        }
        let a = read(&p);
        let dir = root.join("works").join(id);
        let w = read(&dir.join("manifest.json"));
        for t in a["tracks"].as_array().unwrap() {
            let mut times = Vec::new();
            for s in w["songs"].as_array().unwrap() {
                for (i, c) in s["charts"].as_array().unwrap().iter().enumerate() {
                    if !t["chartIds"].as_array().unwrap().contains(&c["id"]) {
                        continue;
                    }
                    let r = read(
                        &dir.join(format!("references/{}-{i}.json", s["id"].as_str().unwrap())),
                    );
                    let ch: Chart = serde_json::from_value(r["sourceChart"].clone()).unwrap();
                    times.extend(distinct_heads(&ch).unwrap());
                }
            }
            times.sort_by(f64::total_cmp);
            times.dedup();
            let got = chart_candidates(&times, &mut || Ok(())).unwrap();
            for f in t["chartCandidates"].as_array().unwrap() {
                let bpm = f["bpm"].as_f64().unwrap();
                let sub = f["subdivisionsPerBeat"].as_u64().unwrap() as u8;
                let nearest = got
                    .iter()
                    .filter(|a| a.subdivisions_per_beat == sub)
                    .min_by(|a, b| (a.bpm - bpm).abs().total_cmp(&(b.bpm - bpm).abs()))
                    .unwrap();
                let fixed = chart_fit(&times, bpm, sub).unwrap();
                let best = got
                    .iter()
                    .filter(|v| v.subdivisions_per_beat == sub)
                    .map(|v| v.coherence)
                    .fold(0.0, f64::max);
                let same = (fixed.coherence - f["coherence"].as_f64().unwrap()).abs() < 1e-8
                    && (fixed.p95_grid_residual_ms - f["p95GridResidualMs"].as_f64().unwrap())
                        .abs()
                        < 1e-5
                    && best + 1e-7 >= f["coherence"].as_f64().unwrap();
                if !same {
                    failures.push(json!({"work":id,"new":nearest,"old":f}));
                } else {
                    checks.push(json!({"work":id,"bpm":nearest.bpm,"sub":sub}));
                }
            }
        }
        eprintln!("lattice-work: {id}");
    }
    println!(
        "{}",
        json!({"schema":"ddi-danoni-rust-lattice-parity/1","fits":checks.len(),"failures":failures,"scope":"Every fixed legacy fit statistic agrees; native peak search preserves or improves maximum coherence, including numerical/boundary peak differences","passed":failures.is_empty()})
    );
    assert!(
        failures.is_empty(),
        "{} lattice differences",
        failures.len()
    );
}
#[test]
fn native_analysis_is_bounded_and_records_reference_differences() {
    let Some(root) = std::env::var_os("DDI_DANONI_TEMPO_GENERATION") else {
        return;
    };
    let Some(media) = std::env::var_os("DDI_DANONI_TEMPO_MEDIA") else {
        return;
    };
    let Some(out) = std::env::var_os("DDI_DANONI_TEMPO_OUTPUT") else {
        return;
    };
    let (root, media, out) = (Path::new(&root), Path::new(&media), Path::new(&out));
    fs::create_dir(out).expect("fresh native-tempo output");
    let generation = read(&root.join("manifest.json"));
    let mut checks = Vec::new();
    let mut failures = Vec::new();
    for row in generation["works"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let dir = root.join("works").join(id);
        let w = read(&dir.join("manifest.json"));
        let work: WorkRequest = serde_json::from_value(w["request"].clone()).unwrap();
        let supplied = work
            .media
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let base = format!("{id}-{i}");
                let pcm = format!("{base}/pcm.wav");
                let receipt = format!("{base}/manifest.json");
                AnalysisPcm {
                    source_audio_sha256: m.source_audio_sha256.clone(),
                    pcm: Binding {
                        namespace: "media".into(),
                        path: pcm.clone(),
                        sha256: sha256(&fs::read(media.join(&pcm)).unwrap()),
                    },
                    receipt: Binding {
                        namespace: "media".into(),
                        path: receipt.clone(),
                        sha256: sha256(&fs::read(media.join(receipt)).unwrap()),
                    },
                }
            })
            .collect();
        let source = Bundle {
            dir: &dir,
            w: &w,
            legacy: None,
            media: Some(media),
            cache: None,
        };
        let request = TempoRequest {
            manifest: work.manifest.clone(),
            media: supplied,
            legacy_analysis: None,
            cached: None,
        };
        let result = prepare_tempo(&request, &source, &mut || Ok(()));
        match result {
            Ok(result) => {
                fs::write(
                    out.join(format!("{id}.json")),
                    serde_json::to_vec_pretty(&result).unwrap(),
                )
                .unwrap();
                let mut charts = Vec::new();
                for s in w["songs"].as_array().unwrap() {
                    for c in s["charts"].as_array().unwrap() {
                        let cid = c["id"].as_str().unwrap();
                        charts.push(json!({"id":cid,"new":result["review"]["charts"][cid],"accepted":c["musicalGrid"]}));
                    }
                }
                checks.push(json!({"work":id,"status":result["status"],"validation":result["validation"],"fallback":result["fallback"],"charts":charts}));
            }
            Err(e) => failures.push(json!({"work":id,"error":e})),
        }
        eprintln!("native-tempo-work: {id}");
    }
    let result = json!({"schema":"ddi-danoni-rust-native-tempo-corpus/1","checks":checks,"failures":failures,"passed":failures.is_empty(),"scope":"Fresh versioned Rust estimates compared openly to accepted grids; musical equality not asserted"});
    fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    println!("{}", result);
    assert!(failures.is_empty());
    assert_eq!(checks.len(), generation["works"].as_array().unwrap().len());
}

#[test]
fn native_results_prepare_complete_songs_with_bounded_times_and_original_scroll() {
    let Some(root) = std::env::var_os("DDI_DANONI_TEMPO_GENERATION") else {
        return;
    };
    let Some(results) = std::env::var_os("DDI_DANONI_TEMPO_RESULTS") else {
        return;
    };
    let (root, results) = (Path::new(&root), Path::new(&results));
    let generation = read(&root.join("manifest.json"));
    let (mut songs, mut charts, mut endpoints, mut velocity_checks) = (0, 0, 0, 0);
    let mut maximum_error = 0f64;
    for row in generation["works"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let dir = root.join("works").join(id);
        let w = read(&dir.join("manifest.json"));
        let result = read(&results.join(format!("{id}.json")));
        let mut request: WorkRequest = serde_json::from_value(w["request"].clone()).unwrap();
        let bytes = serde_json::to_vec(&result["review"]).unwrap();
        request.reviews.grid = if result["status"] == "estimated" {
            Some(Binding {
                namespace: "cache".into(),
                path: "grid.json".into(),
                sha256: sha256(&bytes),
            })
        } else {
            None
        };
        let source = Bundle {
            dir: &dir,
            w: &w,
            legacy: None,
            media: None,
            cache: Some(bytes),
        };
        let prepared = prepare_work(&request, &source, &mut |_| Ok(()))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        for song in prepared.manifest["songs"].as_array().unwrap() {
            songs += 1;
            let path = song["simfile"].as_str().unwrap();
            let artifact = prepared.files.iter().find(|f| f.path == path).unwrap();
            let parsed =
                ddi_chart::formats::sm::parse_ssc(std::str::from_utf8(&artifact.bytes).unwrap())
                    .unwrap();
            let old =
                ddi_chart::formats::sm::parse_ssc(&fs::read_to_string(dir.join(path)).unwrap())
                    .unwrap();
            assert_eq!(parsed.charts.len(), old.charts.len());
            for (index, c) in song["charts"].as_array().unwrap().iter().enumerate() {
                charts += 1;
                let game = &parsed.charts[index];
                let timing = game.timing(&parsed);
                let previous = &old.charts[index];
                let previous_timing = previous.timing(&old);
                assert_eq!(game.layout, previous.layout);
                assert_eq!(game.meter, previous.meter);
                assert_eq!(game.difficulty, previous.difficulty);
                assert_eq!(game.notes.len(), previous.notes.len());
                let mut parsed_endpoints = std::collections::BTreeMap::new();
                for n in &game.notes {
                    parsed_endpoints.insert((n.lane as usize, n.tick.0), timing.seconds_at(n.tick));
                    if let ddi_chart::NoteKind::HoldHead { end } = n.kind {
                        parsed_endpoints.insert((n.lane as usize, end.0), timing.seconds_at(end));
                    }
                }
                let rows = c["endpoints"].as_array().unwrap();
                assert_eq!(parsed_endpoints.len(), rows.len());
                let padding = c["paddingTicks"].as_i64().unwrap();
                for e in rows {
                    let key = (
                        e["lane"].as_u64().unwrap() as usize,
                        e["tick"].as_i64().unwrap() + padding,
                    );
                    let time = parsed_endpoints[&key];
                    assert!((time - e["exportSeconds"].as_f64().unwrap()).abs() < 1e-8);
                    let error = (time - e["sourceSeconds"].as_f64().unwrap()).abs() * 1000.0;
                    maximum_error = maximum_error.max(error);
                    assert!(error <= 9.0 + 1e-7, "{id}: {error} ms");
                    endpoints += 1;
                }
                // The event's quantized time may move slightly; compare velocity
                // inside intervals away from both old and new boundaries.
                let start = timing
                    .seconds_at(ddi_chart::Tick::ZERO)
                    .max(previous_timing.seconds_at(ddi_chart::Tick::ZERO));
                let mut boundaries = timing
                    .scrolls
                    .iter()
                    .map(|e| timing.seconds_at(e.tick))
                    .chain(
                        previous_timing
                            .scrolls
                            .iter()
                            .map(|e| previous_timing.seconds_at(e.tick)),
                    )
                    .filter(|t| *t > start)
                    .collect::<Vec<_>>();
                boundaries.extend([start, 7201.0]);
                boundaries.sort_by(f64::total_cmp);
                boundaries.dedup();
                for interval in boundaries.windows(2).filter(|w| w[1] - w[0] > 0.05) {
                    let time = interval[0] + 0.02;
                    for x in [1.0, 2.0, 3.0] {
                        let options = ddi_engine::scroll::ScrollOptions {
                            speed: ddi_engine::scroll::SpeedMod::XMod(x),
                            ..Default::default()
                        };
                        // Probe near the receptor: subtracting distant f32 note
                        // positions amplifies rounding into a false velocity error.
                        let velocity = |timing: &ddi_chart::TimingMap| {
                            let target = timing.displayed_beat(timing.beat_at(time));
                            let y = |t| {
                                ddi_engine::scroll::ScrollState::at(options, timing, t)
                                    .y_displayed(target, time)
                                    as f64
                            };
                            (y(time) - y(time + 0.01)) / 0.01
                        };
                        let a = velocity(timing);
                        let b = velocity(previous_timing);
                        assert!(
                            (a - b).abs() < 1e-5 * (1.0 + a.abs().max(b.abs())),
                            "{id}: x{x} native velocity changed at {time}: {a} vs {b}"
                        );
                        velocity_checks += 1;
                    }
                }
            }
        }
    }
    println!(
        "{}",
        json!({"schema":"ddi-danoni-native-tempo-consumption/1","songs":songs,"charts":charts,"endpoints":endpoints,"maximumNoteErrorMs":maximum_error,"velocityChecks":velocity_checks,"passed":true,"scope":"Complete Rust preparation consumed new grids; real game parser checks all endpoints and x1/x2/x3 scroll velocities away from quantized event boundaries"})
    );
    assert_eq!(songs, 93);
    assert_eq!(charts, 144);
    assert_eq!(endpoints, 105459);
}
