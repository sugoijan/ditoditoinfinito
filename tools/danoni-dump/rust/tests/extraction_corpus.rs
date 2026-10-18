#![cfg(feature = "native")]
//! Local-only family differential gate; no work code or media is executed.
use ddi_danoni_export::{assets::*, extraction::*, native::DirectorySource};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn read(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
fn bind(path: String, hash: String) -> Binding {
    Binding {
        namespace: "corpus".into(),
        path,
        sha256: hash,
    }
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
#[test]
fn saved_native_families_match_all_accepted_projection_clock_and_motion_fields() {
    let Some(registry) = std::env::var_os("DDI_DANONI_EXTRACTION_REGISTRY") else {
        eprintln!("SKIP: set DDI_DANONI_EXTRACTION_REGISTRY");
        return;
    };
    let registry = read(Path::new(&registry));
    let corpus = PathBuf::from(registry["root"].as_str().unwrap());
    let out = std::env::var_os("DDI_DANONI_EXTRACTION_OUTPUT").map(PathBuf::from);
    if let Some(out) = &out {
        fs::create_dir(out).expect("fresh external projection output");
    }
    let mut checks = Vec::new();
    let mut strict_rejections = Vec::new();
    let mut projection_checks = Vec::new();
    let mut families = BTreeMap::<String, usize>::new();
    let (mut taps, mut holds) = (0, 0);
    for work in registry["works"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["required"] == true)
    {
        let manifest_path = corpus.join(work["source"]["path"].as_str().unwrap());
        let recipe_path = manifest_path.parent().unwrap().join("recipe.json");
        if !recipe_path.exists() {
            continue;
        }
        let recipe = read(&recipe_path);
        if recipe["schema"] != "ddi-native-family-recipe/experimental-1" {
            continue;
        }
        let manifest = read(&manifest_path);
        assert_eq!(
            sha256(&fs::read(&manifest_path).unwrap()),
            work["source"]["sha256"]
        );
        let root = PathBuf::from(recipe["root"].as_str().unwrap());
        let source =
            DirectorySource::new(BTreeMap::from([("corpus".into(), root.clone())])).unwrap();
        let source_manifest = read(&root.join(recipe["source"]["path"].as_str().unwrap()));
        let movie_hash = source_manifest
            .get("sourceSwfSha256")
            .unwrap_or(&source_manifest["swf"]["sha256"])
            .as_str()
            .unwrap();
        for row in recipe["charts"].as_array().unwrap() {
            let id = row["id"].as_str().unwrap();
            let family = row["family"].as_str().unwrap();
            let options:ProjectionOptions=serde_json::from_value(json!({"family":family,"mode":row["mode"],"sourcePrefix":row.get("sourcePrefix").unwrap_or(&json!("")),"sourceSuffix":row["sourceSuffix"],"reviewMissingDecorativeSprites":row.get("reviewMissingDecorativeSprites").unwrap_or(&json!(false))})).unwrap();
            let binding = &row["projection"];
            let relative = binding["path"].as_str().unwrap();
            let snapshot_path = root.join(relative);
            let run_path = Path::new(relative)
                .parent()
                .unwrap()
                .join("run.json")
                .to_string_lossy()
                .into_owned();
            let run = read(&root.join(&run_path));
            let config = fs::read_dir(root.join(recipe["inputs"].as_str().unwrap()))
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| {
                    p.extension().is_some_and(|e| e == "json")
                        && sha256(&fs::read(p).unwrap()) == run["configSha256"]
                })
                .expect("preserved capture config");
            let optional_color = row.get("optionalColorReview").map(|b| {
                let path = b["path"].as_str().unwrap();
                let review = read(&root.join(path));
                OptionalColorEvidence {
                    review: bind(path.into(), b["sha256"].as_str().unwrap().into()),
                    missing_responses: review["missingResources"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| {
                            bind(
                                r["path"].as_str().unwrap().into(),
                                r["sha256"].as_str().unwrap().into(),
                            )
                        })
                        .collect(),
                }
            });
            let request = ProjectionRequest {
                capture: CaptureBindings {
                    snapshot: bind(relative.into(), binding["sha256"].as_str().unwrap().into()),
                    run: bind(run_path, binding["runSha256"].as_str().unwrap().into()),
                    config: bind(
                        config
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        run["configSha256"].as_str().unwrap().into(),
                    ),
                },
                movie: bind(
                    Path::new(recipe["inputs"].as_str().unwrap())
                        .join(
                            read(&snapshot_path)["source"]["config"]["movie"]
                                .as_str()
                                .unwrap(),
                        )
                        .to_string_lossy()
                        .into_owned(),
                    movie_hash.into(),
                ),
                options,
                optional_color,
            };
            let result = match prepare_projection(&request, &source, &mut || Ok(())) {
                Ok(result) => result,
                Err(error) if error.code == "native-source-bank-missing" => {
                    strict_rejections.push(json!({"work":work["id"],"chart":id,"family":family,"error":error,"snapshotSha256":request.capture.snapshot.sha256,"scope":"Current Rust and Python contracts reject source speed without a native bank; frozen accepted reference predates that check"}));
                    continue;
                }
                Err(error) => panic!("{} {id}: {error}", work["id"]),
            };
            let current_oracle = std::env::var_os("DDI_DANONI_EXTRACTION_ORACLE")
                .map(|p| {
                    PathBuf::from(p).join(format!("{}-{id}.json", work["id"].as_str().unwrap()))
                })
                .filter(|p| p.exists())
                .map(|p| read(&p));
            if let Some(oracle) = &current_oracle {
                compare(
                    &result["projection"],
                    &oracle["projection"],
                    "current Python projection",
                );
                compare(
                    &result["preparedIteration"],
                    &oracle["preparedIteration"],
                    "current Python iteration",
                );
            }
            let chart = manifest["charts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == id)
                .unwrap();
            let reference = read(
                &manifest_path
                    .parent()
                    .unwrap()
                    .join(chart["nativeReference"].as_str().unwrap()),
            );
            let mut comparable_projection = result["projection"].clone();
            let mut added_projection_fields = Vec::new();
            for key in [
                "initialPixelSpeed",
                "compiledIteration",
                "clockInitializationIteration",
                "inactiveSpeedTail",
                "inactiveSourceSpeed",
            ] {
                if reference["projection"].get(key).is_none()
                    && comparable_projection.get(key).is_some()
                {
                    comparable_projection.as_object_mut().unwrap().remove(key);
                    added_projection_fields.push(key);
                }
            }
            compare(
                &comparable_projection,
                &reference["projection"],
                &format!("{} {id}", work["id"]),
            );
            assert_eq!(result["preparedIteration"], reference["preparedIteration"]);
            projection_checks.push(json!({"work":work["id"],"chart":id,"family":family,"passed":true,"newExplicitFields":added_projection_fields}));
            let clean = |binding: &Value| {
                let capture = read(&root.join(binding["path"].as_str().unwrap()));
                if let Some(review) = &request.optional_color {
                    review_optional_colors(&capture, &read(&root.join(&review.review.path)))
                        .unwrap()
                        .0
                } else {
                    capture
                }
            };
            let motion_capture = clean(&row["motionCapture"]);
            let selection: MotionSelection = serde_json::from_value(row["motion"].clone()).unwrap();
            let clock = result["projection"]["clockField"].as_str();
            let motion = match movie_observation(
                &motion_capture,
                &result["projection"],
                &selection,
                clock,
                &mut || Ok(()),
            ) {
                Ok(motion) => motion,
                Err(error) if error.code == "native-motion-segment-unproved" => {
                    strict_rejections.push(json!({"work":work["id"],"chart":id,"family":family,"stage":"motion","error":error,"scope":"Current Rust and Python motion contracts require one positive author segment; original frozen motion predates the strengthened units check"}));
                    continue;
                }
                Err(error) => panic!("{} {id} motion: {error}", work["id"]),
            };
            if let Some(oracle) = &current_oracle {
                compare(&motion, &oracle["motion"], "current Python motion");
            }
            let mut comparable_motion = motion.clone();
            let mut added_motion_fields = Vec::new();
            for key in ["observedAuthorMultiplier", "stagePositionTolerancePixels"] {
                if reference["motion"].get(key).is_none() {
                    comparable_motion.as_object_mut().unwrap().remove(key);
                    added_motion_fields.push(key);
                }
            }
            compare(
                &comparable_motion,
                &reference["motion"],
                &format!("{} {id} motion", work["id"]),
            );
            let audio = clean(&row["audioCapture"]);
            let origin = movie_audio_origin(&audio, clock, 0).unwrap();
            compare(
                &origin,
                &reference["audioOrigin"],
                &format!("{} {id} origin", work["id"]),
            );
            if let Some(oracle) = &current_oracle {
                compare(&origin, &oracle["audioOrigin"], "current Python origin");
            }
            validate_clock_agreement(&origin, &motion).unwrap();
            let relative = relative_chart(
                &result["projection"],
                origin["logicalFrame"].as_f64().unwrap(),
            )
            .unwrap();
            for key in ["lanes", "speed", "boost", "colors", "stops"] {
                if chart.get(key).is_some() {
                    compare(
                        &relative[key],
                        &chart[key],
                        &format!("{} {id} relative {key}", work["id"]),
                    );
                }
            }
            for lane in relative["lanes"].as_array().unwrap() {
                taps += lane["tapsSeconds"].as_array().unwrap().len();
                holds += lane["holdsSeconds"].as_array().unwrap().len();
            }
            *families.entry(family.into()).or_default() += 1;
            if let Some(out) = &out {
                fs::write(out.join(format!("{}-{id}.json",work["id"].as_str().unwrap())),serde_json::to_vec_pretty(&json!({"projection":result,"motion":motion,"audioOrigin":origin,"relativeChart":relative})).unwrap()).unwrap();
            }
            checks.push(json!({"work":work["id"],"chart":id,"family":family,"snapshotSha256":sha256(&fs::read(snapshot_path).unwrap()),"newExplicitMotionFields":added_motion_fields,"newExplicitProjectionFields":added_projection_fields,"passed":true}));
        }
        eprintln!("projection-work: {}", work["id"]);
    }
    assert_eq!(checks.len(), 115);
    assert_eq!(projection_checks.len(), 116);
    assert_eq!(strict_rejections.len(), 1);
    let report = json!({"schema":"ddi-danoni-rust-native-projection-parity/1","checks":checks,"projectionChecks":projection_checks,"strictRejections":strict_rejections,"families":families,"taps":taps,"holds":holds,"passed":true,"scope":"Saved structural-family states/run/config bindings, every projection field, movement and logical stream origin; no SWF demux, fresh execution or decoded audio alignment claimed"});
    if let Some(out) = out {
        fs::write(
            out.join("manifest.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    println!("{report}");
}
