#![cfg(feature = "native")]
use ddi_danoni_export::{Error, extraction::text, model::Mode, profiles::*};
use serde_json::{Value, json};
fn string(s: &str) -> Value {
    json!({"type":"string","text":"untrusted display text","utf16":s.encode_utf16().collect::<Vec<_>>(),"truncated":false})
}
fn stored(v: &Value, objects: &mut Vec<Value>) -> Value {
    if let Some(s) = v.as_str() {
        return string(s);
    }
    if !v.is_object() && !v.is_array() {
        return v.clone();
    }
    let id = objects.len();
    objects.push(Value::Null);
    let mut props = vec![];
    if let Some(a) = v.as_array() {
        props.push(json!({"name":string("length"),"value":a.len()}));
        for (i, v) in a.iter().enumerate() {
            props.push(json!({"name":string(&i.to_string()),"value":stored(v,objects)}));
        }
    } else {
        for (k, v) in v.as_object().unwrap() {
            props.push(json!({"name":string(k),"value":stored(v,objects)}));
        }
    }
    objects[id] = json!({"id":id,"properties":props,"truncated":false});
    json!({"ref":id})
}
fn menu(selected: usize) -> Value {
    let labels = ["普通", "Hard", "Special"];
    let keys = ["5", "7", "9i"];
    let root = json!({"difficulty":3,"diff_label":labels,"key_label":keys,"dif_shift":selected,"diffi":labels[selected],"keyLabel":keys[selected],"diffin":if selected==0 {json!("")} else {json!(selected+1)}});
    let mut objects = vec![];
    let reference = stored(&root, &mut objects);
    let captures = [45,46].map(|frame| json!({"frame":frame,"state":{"objects":objects,"roots":[{"level":0,"value":reference,"movieFrame":3}],"displayTree":[],"incomplete":false,"avm1Halted":false}}));
    json!({"swfHeaderFps":60,"avm1Halted":false,"audit":{"entries":[],"truncated":false,"runtimeErrors":0,"droppedEntries":0,"truncatedDetails":0},"captures":captures})
}
fn set_property(capture: &mut Value, frame: usize, key: &str, value: Value) {
    let p = capture["captures"][frame]["state"]["objects"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| text(&p["name"]).unwrap() == key)
        .unwrap();
    p["value"] = value;
}
#[test]
fn menu_uses_stored_labels_and_preserves_unsupported_modes() {
    let m = read_menu(&menu(1)).unwrap();
    assert_eq!(m.selected, 1);
    assert_eq!(m.rows[0].label, "普通");
    assert_eq!(m.rows[0].mode, Some(Mode::Five));
    assert_eq!(m.rows[1].mode, Some(Mode::Seven));
    assert_eq!(m.rows[2].mode, None);
    assert!(options(Mode::NineA).is_err());
    assert!(options(Mode::NineB).is_err());
}
#[test]
fn inconsistent_unbounded_or_incomplete_menus_are_rejected() {
    for kind in [
        "fps",
        "halted",
        "root",
        "order",
        "bound",
        "count",
        "selector",
        "suffix",
        "label",
        "change",
        "hole",
        "duplicate",
        "incomplete",
        "audit",
    ] {
        let mut c = menu(1);
        match kind {
            "fps" => c["swfHeaderFps"] = json!(30),
            "halted" => c["avm1Halted"] = json!(true),
            "root" => c["captures"][1]["state"]["roots"][0]["movieFrame"] = json!(4),
            "order" => c["captures"][1]["frame"] = json!(45),
            "bound" => c["captures"][1]["frame"] = json!(1201),
            "count" => set_property(&mut c, 0, "difficulty", json!(17)),
            "selector" => set_property(&mut c, 0, "dif_shift", json!(3)),
            "suffix" => set_property(&mut c, 0, "diffin", json!(1)),
            "label" => set_property(&mut c, 0, "diffi", string("fake")),
            "change" => {
                c["captures"][1] = menu(0)["captures"][1].clone();
            }
            "hole" => {
                c["captures"][0]["state"]["objects"][1]["properties"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            "duplicate" => {
                let p = c["captures"][0]["state"]["objects"][0]["properties"][0].clone();
                c["captures"][0]["state"]["objects"][0]["properties"]
                    .as_array_mut()
                    .unwrap()
                    .push(p);
            }
            "incomplete" => c["captures"][0]["state"]["incomplete"] = json!(true),
            "audit" => c["audit"]["truncated"] = json!(true),
            _ => unreachable!(),
        }
        assert!(read_menu(&c).is_err(), "{kind}");
    }
}
#[test]
fn controls_cycle_from_observed_selection_and_leave_stable_menu_snapshots() {
    let m = read_menu(&menu(1)).unwrap();
    let c = selector_controls(&m, 0).unwrap();
    assert_eq!(c.events.len(), 4);
    assert_eq!(c.events[1]["frame"], 43);
    assert_eq!(c.events[2]["frame"], 45);
    assert_eq!(c.start, 50);
    assert_eq!(c.initialization_frames, (48..=58).collect::<Vec<_>>());
    assert_eq!(selector_controls(&m, 1).unwrap().events.len(), 2);
    assert!(selector_controls(&m, 3).is_err());
    let mut large = m;
    large.rows = vec![large.rows[0].clone(); 16];
    large.selected = 0;
    let c = selector_controls(&large, 15).unwrap();
    assert_eq!(c.start, 76);
    assert_eq!(c.events[c.events.len() - 2]["frame"], 71);
    assert_eq!(c.initialization_frames[0], 74);
}
fn chart(mode: &str) -> Value {
    let count = if mode == "7" { 7 } else { 5 };
    let lanes = (0..count)
        .map(|i| json!({"nativeFrames":if i==count-1 {vec![400.]} else {vec![]}}))
        .collect::<Vec<_>>();
    json!({"mode":mode,"clockField":"frame_num","constantTravelFrames":100,"lanes":lanes,"speed":[]})
}
#[test]
fn motion_proposals_use_clock_offset_sparse_lanes_and_seven_key_order() {
    let origin = json!({"logicalFrame":1,"iteration":201});
    let proposals = motion_candidates(&chart("7"), &origin, &mut || Ok(())).unwrap();
    assert_eq!(proposals.len(), 3);
    assert_eq!(proposals[0].lane, 6);
    assert_eq!(proposals[0].note, "ri_s0");
    assert_eq!(proposals[0].receptor, "ri_step");
    assert_eq!(proposals[0].iterations, (550..=555).collect::<Vec<_>>());
    assert_eq!(proposals[0].geometry_reference.as_deref(), Some("le_step"));
    assert!(proposals[0].target_y.is_none());
    assert_eq!(
        motion_candidates(&chart("5"), &origin, &mut || Ok(())).unwrap()[0].note,
        "oni_s0"
    );
}
#[test]
fn motion_proposals_avoid_speed_boundaries_zero_speed_and_out_of_budget_notes() {
    let origin = json!({"logicalFrame":1,"iteration":201});
    let mut c = chart("5");
    c["speed"] = json!([{"nativeFrame":352,"multiplier":2}]);
    let proposals = motion_candidates(&c, &origin, &mut || Ok(())).unwrap();
    assert_eq!(proposals.len(), 2);
    assert!(
        !proposals
            .iter()
            .any(|p| p.iterations == vec![550, 551, 552, 553, 554, 555])
    );
    c["speed"] = json!([{"nativeFrame":0,"multiplier":0}]);
    assert!(
        motion_candidates(&c, &origin, &mut || Ok(()))
            .unwrap()
            .is_empty()
    );
    c["speed"] = json!([]);
    c["lanes"][4]["nativeFrames"] = json!([10000]);
    assert!(
        motion_candidates(&c, &origin, &mut || Ok(()))
            .unwrap()
            .is_empty()
    );
    c["constantTravelFrames"] = json!(0);
    assert!(motion_candidates(&c, &origin, &mut || Ok(())).is_err());
    assert_eq!(
        motion_candidates(&chart("5"), &origin, &mut || Err(Error {
            code: "cancelled",
            message: "test".into()
        }))
        .unwrap_err()
        .code,
        "cancelled"
    );
}
#[test]
fn source_context_cannot_smuggle_manual_selector_or_observation_recipes() {
    let c = json!({"movie":"movie.swf","sourceUrl":"https://example.test/song.swf","resources":[]});
    assert!(serde_json::from_value::<NativeContext>(c.clone()).is_ok());
    for name in [
        "events",
        "captureFrames",
        "render",
        "mixAudio",
        "avmExecutionMs",
    ] {
        let mut c = c.clone();
        c[name] = json!([]);
        assert!(
            serde_json::from_value::<NativeContext>(c).is_err(),
            "{name}"
        );
    }
}

#[test]
fn unknown_movie_or_early_cancellation_does_not_launch_worker_or_create_output() {
    use ddi_danoni_export::{assets::*, supervisor::WorkerSpec};
    let mut movie = b"FWS\x06".to_vec();
    movie.extend(18u32.to_le_bytes());
    movie.extend([8, 0, 0, 60, 1, 0, 64, 0, 0, 0]);
    struct Source(Vec<u8>);
    impl AssetSource for Source {
        fn read(&self, _: &str, path: &str, max: usize) -> ddi_danoni_export::Result<Vec<u8>> {
            assert_eq!(path, "movie.swf");
            assert!(self.0.len() <= max);
            Ok(self.0.clone())
        }
    }
    let b = Binding {
        namespace: "source".into(),
        path: "movie.swf".into(),
        sha256: sha256(&movie),
    };
    let request: ProfileRequest =
        serde_json::from_value(json!({"movie":b,"context":b,"sourceManifest":b})).unwrap();
    let worker:WorkerSpec=serde_json::from_value(json!({"executable":"/not/a/worker","sha256":"0".repeat(64),"buildIdentity":"/not/a/build","buildIdentitySha256":"0".repeat(64)})).unwrap();
    let output = std::env::temp_dir().join(format!("ddi-profile-unknown-{}", std::process::id()));
    assert!(!output.exists());
    let source = Source(movie);
    assert_eq!(
        execute(&request, &worker, &source, &output, &mut |_| Ok(()))
            .unwrap_err()
            .code,
        "unsupported-native-profile"
    );
    assert!(!output.exists());
    assert_eq!(
        execute(&request, &worker, &source, &output, &mut |_| Err(Error {
            code: "cancelled",
            message: "test".into()
        }))
        .unwrap_err()
        .code,
        "cancelled"
    );
    assert!(!output.exists());
}
#[test]
fn installed_profile_executes_all_selectors_and_blocks_late_cancellation_and_limits() {
    use std::{fs, path::PathBuf};
    let (Some(job), Some(bundle)) = (
        std::env::var_os("DDI_DANONI_PROFILE_JOB"),
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
    ) else {
        eprintln!("SKIP: set DDI_DANONI_PROFILE_JOB and DDI_DANONI_WORKER_BUNDLE");
        return;
    };
    let scratch = std::env::temp_dir().join(format!("ddi-profile-worker-{}", std::process::id()));
    fs::create_dir(&scratch).unwrap();
    struct Guard(PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _guard = Guard(scratch.clone());
    let bundle = PathBuf::from(bundle);
    let job = PathBuf::from(job);
    let out = execute_job(&bundle, &job, &scratch.join("complete"), &mut |_| Ok(())).unwrap();
    assert!(out.ready, "{:?}", out.failure);
    let rows = out.manifest["menu"]["menu"]["rows"].as_array().unwrap();
    assert_eq!(
        out.manifest["normalized"]["charts"].as_u64().unwrap() as usize,
        rows.iter().filter(|r| !r["mode"].is_null()).count()
    );
    let replay: ddi_danoni_export::orchestration::PlanJob =
        serde_json::from_slice(&fs::read(out.output.join("replay-job.json")).unwrap()).unwrap();
    let source = ddi_danoni_export::native::DirectorySource::new(std::collections::BTreeMap::from(
        [("profile".into(), out.output.clone())],
    ))
    .unwrap();
    ddi_danoni_export::orchestration::prepare_plan(&replay.plan, &source, &mut || Ok(())).unwrap();
    let cancelled = execute_job(&bundle, &job, &scratch.join("cancelled"), &mut |p| {
        if p.stage == "ready" {
            Err(Error {
                code: "cancelled",
                message: "late test".into(),
            })
        } else {
            Ok(())
        }
    })
    .unwrap();
    assert!(!cancelled.ready);
    assert_eq!(cancelled.failure.unwrap().code, "cancelled");
    assert!(!cancelled.output.join("manifest.json").exists());
    assert!(cancelled.output.join("failure.json").is_file());
    assert!(cancelled.manifest["normalized"].is_null());
    let mut limited: ProfileJob = serde_json::from_slice(&fs::read(&job).unwrap()).unwrap();
    // Preserve original job-relative roots when relocating this local test job.
    for root in limited.roots.values_mut() {
        if root.is_relative() {
            *root = job.parent().unwrap().join(&*root);
        }
    }
    limited.request.limits.captures = 1;
    let path = scratch.join("limited.json");
    fs::write(&path, serde_json::to_vec(&limited).unwrap()).unwrap();
    let out = execute_job(&bundle, &path, &scratch.join("limited"), &mut |_| Ok(())).unwrap();
    assert!(!out.ready);
    assert_eq!(out.failure.unwrap().code, "native-profile-limit");
    assert!(!out.output.join("manifest.json").exists());
    // A missing original GET is retained for the host; no browser/network or
    // fabricated DOS response is introduced by profile discovery.
    use ddi_danoni_export::{assets::*, native::DirectorySource};
    let original = DirectorySource::new(limited.roots.clone()).unwrap();
    let b = &limited.request.context;
    let mut context: NativeContext =
        serde_json::from_slice(&original.read(&b.namespace, &b.path, 65536).unwrap()).unwrap();
    context.resources.clear();
    context.movie = "movie.swf".into();
    let missing_source = scratch.join("missing-source");
    fs::create_dir(&missing_source).unwrap();
    let b = &limited.request.movie;
    fs::write(
        missing_source.join("movie.swf"),
        original
            .read(&b.namespace, &b.path, 16 * 1024 * 1024)
            .unwrap(),
    )
    .unwrap();
    let raw = serde_json::to_vec(&context).unwrap();
    fs::write(missing_source.join("context.json"), &raw).unwrap();
    limited.roots.insert("missing".into(), missing_source);
    limited.request.context = Binding {
        namespace: "missing".into(),
        path: "context.json".into(),
        sha256: sha256(&raw),
    };
    limited.request.limits = ProfileLimits::default();
    let path = scratch.join("missing.json");
    fs::write(&path, serde_json::to_vec(&limited).unwrap()).unwrap();
    let out = execute_job(&bundle, &path, &scratch.join("missing"), &mut |_| Ok(())).unwrap();
    assert!(!out.ready);
    assert_eq!(
        out.failure.unwrap().code,
        "native-profile-needs-dependencies"
    );
    assert!(
        !out.manifest["pendingDependencies"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!out.output.join("manifest.json").exists());
}

#[test]
fn earlier_hold_witnesses_precede_taps_and_work_on_hold_only_charts() {
    use ddi_danoni_export::extraction::MotionSource;
    let mut c = chart("7");
    c["lanes"][2]["nativeHoldFrames"] = json!([[250, 300]]);
    let origin = json!({"logicalFrame":1,"iteration":201});
    let proposals = motion_candidates(&c, &origin, &mut || Ok(())).unwrap();
    assert_eq!(proposals[0].source, MotionSource::HoldHead);
    assert_eq!(proposals[0].note, "fdo_s0");
    assert_eq!(proposals[0].lane, 2);
    assert_eq!(proposals[0].iterations, (400..=405).collect::<Vec<_>>());
    c["lanes"][6]["nativeFrames"] = json!([]);
    assert!(
        motion_candidates(&c, &origin, &mut || Ok(()))
            .unwrap()
            .iter()
            .all(|s| s.source == MotionSource::HoldHead)
    );
    c["lanes"][6]["nativeFrames"] = json!([200]);
    assert_eq!(
        motion_candidates(&c, &origin, &mut || Ok(())).unwrap()[0].source,
        MotionSource::Tap
    );
}

#[test]
fn selector_filters_reject_invalid_indices_before_asset_reads() {
    use ddi_danoni_export::{assets::*, supervisor::WorkerSpec};
    struct NoReads;
    impl AssetSource for NoReads {
        fn read(&self, _: &str, _: &str, _: usize) -> ddi_danoni_export::Result<Vec<u8>> {
            panic!("Invalid selectors must not read assets")
        }
    }
    let b = json!({"namespace":"source","path":"movie.swf","sha256":"0".repeat(64)});
    let base = json!({"movie":b,"sourceManifest":b,"context":b});
    let old: ProfileRequest = serde_json::from_value(base.clone()).unwrap();
    assert!(old.selectors.is_none());
    assert!(
        serde_json::to_value(old)
            .unwrap()
            .get("selectors")
            .is_none()
    );
    let worker:WorkerSpec=serde_json::from_value(json!({"executable":"/not/a/worker","sha256":"0".repeat(64),"buildIdentity":"/not/a/build","buildIdentitySha256":"0".repeat(64)})).unwrap();
    let output = std::env::temp_dir().join(format!("ddi-invalid-selectors-{}", std::process::id()));
    for indices in [vec![], vec![0, 0], vec![1, 0], vec![16]] {
        let mut v = base.clone();
        v["selectors"] = json!(indices);
        let r: ProfileRequest = serde_json::from_value(v).unwrap();
        assert_eq!(
            execute(&r, &worker, &NoReads, &output, &mut |_| Ok(()))
                .unwrap_err()
                .code,
            "invalid-native-profile"
        );
        assert!(!output.exists());
    }
}

fn input_chart() -> Value {
    let mut c = chart("5");
    c["nativeLaneIndices"] = json!([0, 2, 4, 6, 3]);
    c["nativeKeyCodes"] = json!([37, 300, 40, 32, 38, 301, 39]);
    c["lanes"][0]["nativeHoldFrames"] = json!([[200, 240], [238, 260]]);
    c
}
#[test]
fn hold_intro_inputs_use_native_key_indices_and_merge_overlapping_intervals() {
    let c = input_chart();
    let origin = json!({"logicalFrame":1,"iteration":201});
    assert!(has_early_holds(&c).unwrap());
    let keys = early_hold_inputs(&c, &origin, 58, 62, &mut || Ok(())).unwrap();
    assert_eq!(
        keys,
        json!([{"type":"key","frame":400,"code":"ArrowLeft","holdFrames":62}])
            .as_array()
            .unwrap()
            .clone()
    );
    let mut c = c;
    c["lanes"][0]["nativeHoldFrames"] = json!([[500, 520]]);
    assert!(!has_early_holds(&c).unwrap());
    assert!(
        early_hold_inputs(&c, &origin, 58, 62, &mut || Ok(()))
            .unwrap()
            .is_empty()
    );
}
#[test]
fn derived_play_inputs_never_guess_keys_clocks_or_raise_worker_bounds() {
    let origin = json!({"logicalFrame":1,"iteration":201});
    for bad in [
        "key",
        "index",
        "fractional",
        "initialization",
        "long",
        "slots",
        "past-end",
    ] {
        let mut c = input_chart();
        let mut after = 58;
        let mut slots = 62;
        match bad {
            "key" => c["nativeKeyCodes"][0] = json!(65),
            "index" => c["nativeLaneIndices"][0] = json!(-1),
            "fractional" => c["lanes"][0]["nativeHoldFrames"] = json!([[200.5, 240]]),
            "initialization" => after = 400,
            "long" => c["lanes"][0]["nativeHoldFrames"] = json!([[200, 400]]),
            "slots" => slots = 0,
            "past-end" => {
                c["lanes"][0]["nativeHoldFrames"] = json!([[900, 1002]]);
                c["lanes"][4]["nativeFrames"] = json!([1100]);
            }
            _ => unreachable!(),
        }
        assert!(
            early_hold_inputs(&c, &origin, after, slots, &mut || Ok(())).is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        early_hold_inputs(&input_chart(), &origin, 58, 62, &mut || Err(Error {
            code: "cancelled",
            message: "test".into()
        }))
        .unwrap_err()
        .code,
        "cancelled"
    );
}

#[test]
fn installed_hold_profile_reproves_shared_inputs_and_keeps_unselected_rows_explicit() {
    use std::{fs, path::PathBuf};
    let (Some(job), Some(bundle)) = (
        std::env::var_os("DDI_DANONI_HOLD_PROFILE_JOB"),
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
    ) else {
        eprintln!("SKIP: set DDI_DANONI_HOLD_PROFILE_JOB and DDI_DANONI_WORKER_BUNDLE");
        return;
    };
    let job = PathBuf::from(job);
    let bundle = PathBuf::from(bundle);
    let scratch = std::env::temp_dir().join(format!("ddi-hold-profile-{}", std::process::id()));
    fs::create_dir(&scratch).unwrap();
    struct Guard(PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _guard = Guard(scratch.clone());
    let out = execute_job(&bundle, &job, &scratch.join("ready"), &mut |_| Ok(())).unwrap();
    assert!(out.ready, "{:?}", out.failure);
    let rows = out.manifest["menu"]["menu"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(out.manifest["normalized"]["charts"], 2);
    let excluded = out.manifest["excludedSelectors"].as_array().unwrap();
    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0]["row"]["index"], 1);
    assert_eq!(excluded[0]["reason"], "unselected-selector");
    let trial = &out.manifest["playInputs"][0];
    assert_eq!(trial["selector"], 0);
    assert_eq!(
        trial["seedNormalizationFailure"]["code"],
        "insufficient-audio-origin"
    );
    assert_eq!(trial["projectionUnchanged"], true);
    assert_eq!(trial["audioOriginUnchanged"], true);
    assert_eq!(trial["motionProved"], true);
    let request: ddi_danoni_export::extraction::NativeWorkRequest =
        serde_json::from_slice(&fs::read(out.output.join("normalization-request.json")).unwrap())
            .unwrap();
    let chart = request.charts.iter().find(|c| c.id == "chart-0").unwrap();
    assert_eq!(
        chart.motion_selection.source,
        ddi_danoni_export::extraction::MotionSource::HoldHead
    );
    let events = [&chart.projection, &chart.motion, &chart.audio].map(|c| {
        let config: Value =
            serde_json::from_slice(&fs::read(out.output.join(&c.config.path)).unwrap()).unwrap();
        config["events"].clone()
    });
    assert_eq!(events[0], events[1]);
    assert_eq!(events[1], events[2]);
    assert!(
        events[0]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "key")
    );
    let cancelled = execute_job(&bundle, &job, &scratch.join("cancelled"), &mut |p| {
        if p.stage == "play-inputs" {
            Err(Error {
                code: "cancelled",
                message: "before derived-input recapture".into(),
            })
        } else {
            Ok(())
        }
    })
    .unwrap();
    assert!(!cancelled.ready);
    assert_eq!(cancelled.failure.unwrap().code, "cancelled");
    assert!(!cancelled.output.join("manifest.json").exists());
    assert_eq!(cancelled.manifest["playInputs"][0]["motionProved"], false);

    // A syntactically valid index is still bounded by the observed menu.
    let mut invalid: Value = serde_json::from_slice(&fs::read(&job).unwrap()).unwrap();
    let job_parent = job.parent().unwrap();
    for root in invalid["roots"].as_object_mut().unwrap().values_mut() {
        let path = PathBuf::from(root.as_str().unwrap());
        if path.is_relative() {
            *root = json!(job_parent.join(path));
        }
    }
    invalid["request"]["selectors"] = json!([15]);
    let invalid_job = scratch.join("invalid-job.json");
    fs::write(&invalid_job, serde_json::to_vec(&invalid).unwrap()).unwrap();
    let invalid = execute_job(&bundle, &invalid_job, &scratch.join("invalid"), &mut |_| {
        Ok(())
    })
    .unwrap();
    assert!(!invalid.ready);
    assert_eq!(invalid.failure.unwrap().code, "invalid-native-profile");
    assert_eq!(
        invalid.manifest["menu"]["menu"]["rows"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(invalid.manifest["captures"].as_object().unwrap().len(), 1);
    assert!(!invalid.output.join("manifest.json").exists());
}

#[test]
fn early_menu_contract_has_its_own_frame_and_admits_only_proved_five_key_rows() {
    let early = EngineProfile::RekuConstantMenuEarly;
    let later = EngineProfile::RekuConstantMenu;
    let mut c = menu(0);
    assert!(early.read_menu(&c).is_err());
    for capture in c["captures"].as_array_mut().unwrap() {
        capture["state"]["roots"][0]["movieFrame"] = json!(4);
    }
    assert!(later.read_menu(&c).is_err());
    let observed = early.read_menu(&c).unwrap();
    assert_eq!(observed.rows[0].mode, Some(Mode::Five));
    assert_eq!(observed.rows[1].key_label, "7");
    assert_eq!(observed.rows[1].mode, None);
    assert_eq!(observed.rows[2].mode, None);
    // Sharing a menu decoder must not bypass its other integrity checks.
    set_property(&mut c, 1, "diffin", string("2"));
    assert!(early.read_menu(&c).is_err());
}

#[test]
fn early_controls_use_reviewed_positions_with_observed_selector_wraparound() {
    let mut m = read_menu(&menu(0)).unwrap();
    m.selected = 2;
    let early = EngineProfile::RekuConstantMenuEarly
        .selector_controls(&m, 1)
        .unwrap();
    let later = selector_controls(&m, 1).unwrap();
    assert_eq!(early.events.len(), 4);
    assert_eq!(
        early.events[0],
        json!({"frame":40,"type":"click","x":160,"y":165})
    );
    assert_eq!(
        early.events[1],
        json!({"frame":43,"type":"click","x":40,"y":45})
    );
    assert_eq!(
        early.events[2],
        json!({"frame":45,"type":"click","x":40,"y":45})
    );
    assert_eq!(early.events[3], later.events[3]);
    assert_eq!(early.start, later.start);
    assert_eq!(early.initialization_frames, later.initialization_frames);
    assert_eq!(later.events[0]["y"], 200);
    assert_eq!(later.events[1]["y"], 35);
}

#[test]
fn installed_early_profile_discovers_collaborator_rows_and_replays_rendered_roles() {
    use std::{fs, path::PathBuf};
    let (Some(job), Some(bundle)) = (
        std::env::var_os("DDI_DANONI_EARLY_PROFILE_JOB"),
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
    ) else {
        eprintln!("SKIP: set DDI_DANONI_EARLY_PROFILE_JOB and DDI_DANONI_WORKER_BUNDLE");
        return;
    };
    let scratch = std::env::temp_dir().join(format!("ddi-early-profile-{}", std::process::id()));
    fs::create_dir(&scratch).unwrap();
    struct Guard(PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _guard = Guard(scratch.clone());
    let out = execute_job(
        &PathBuf::from(bundle),
        &PathBuf::from(job),
        &scratch.join("run"),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert!(out.ready, "{:?}", out.failure);
    assert_eq!(out.manifest["profile"], "reku-constant-menu-early/1");
    let menu = &out.manifest["menu"]["menu"];
    assert_eq!(menu["rows"].as_array().unwrap().len(), 4);
    assert_eq!(out.manifest["normalized"]["charts"], 4);
    assert!(
        out.manifest["excludedSelectors"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let request: ddi_danoni_export::extraction::NativeWorkRequest =
        serde_json::from_slice(&fs::read(out.output.join("normalization-request.json")).unwrap())
            .unwrap();
    for (i, chart) in request.charts.iter().enumerate() {
        assert_eq!(chart.id, format!("chart-{i}"));
        assert_eq!(
            chart.label.as_ref().unwrap(),
            menu["rows"][i]["label"].as_str().unwrap()
        );
        assert_eq!(chart.options.mode, Mode::Five);
        let mut events = None;
        for role in [&chart.projection, &chart.motion, &chart.audio] {
            let config: Value =
                serde_json::from_slice(&fs::read(out.output.join(&role.config.path)).unwrap())
                    .unwrap();
            assert_eq!(config["render"], true);
            assert_eq!(config["inspectionScale"], 4);
            assert_eq!(config["avmExecutionMs"], 2000);
            if let Some(events) = &events {
                assert_eq!(&config["events"], events);
            }
            events = Some(config["events"].clone());
        }
    }
    let normalized: Value =
        serde_json::from_slice(&fs::read(out.output.join("normalized/manifest.json")).unwrap())
            .unwrap();
    for chart in normalized["charts"].as_array().unwrap() {
        assert!(
            chart["compiledIteration"].as_u64().unwrap()
                < chart["preparedIteration"].as_u64().unwrap()
        );
        assert_eq!(
            chart["clockInitializationIteration"],
            chart["preparedIteration"]
        );
    }
    let replay: ddi_danoni_export::orchestration::PlanJob =
        serde_json::from_slice(&fs::read(out.output.join("replay-job.json")).unwrap()).unwrap();
    for c in replay.plan.charts {
        assert!(c.projection.render && c.motion.render && c.audio.render);
    }
}
fn scheduled_capture(root: Value) -> Value {
    let mut objects = vec![];
    let reference = stored(&root, &mut objects);
    json!({"captures":[{"frame":55,"state":{"objects":objects,"roots":[{"level":0,"value":reference,"movieFrame":5}],"displayTree":[],"incomplete":false,"avm1Halted":false}}]})
}
fn scheduled_chart() -> Value {
    json!({"mode":"5","clockField":"frame_num","preparedIteration":55,"scheduleAgreement":true,"nativeLaneIndices":[0,2,4,6,3],"nativeSchedules":{"taps":[{"frame":200,"lane":0,"count":1}],"holds":[{"frame":100,"lane":0,"count":1}]},"speed":[],"lanes":(0..5).map(|i|if i==0 {json!({"nativeFrames":[300],"nativeHoldFrames":[[250,280]]})}else{json!({"nativeFrames":[],"nativeHoldFrames":[]})}).collect::<Vec<_>>()})
}
#[test]
fn variable_travel_uses_each_first_head_schedule_and_preserves_reference() {
    let p = EngineProfile::RekuScheduledMenu;
    let c = scheduled_capture(json!({"frame_num":0,"judgeY":{"200":100,"100":150}}));
    let chart = scheduled_chart();
    let before = chart.clone();
    let candidates = p
        .motion_candidates(
            &c,
            &chart,
            &json!({"logicalFrame":145,"iteration":201}),
            &mut || Ok(()),
        )
        .unwrap();
    assert_eq!(candidates[0].note, "fle_s0");
    assert_eq!(candidates[0].iterations, vec![231, 232, 233, 234, 235, 236]);
    assert!(
        candidates
            .iter()
            .any(|c| c.note == "le_s0" && c.iterations == vec![306, 307, 308, 309, 310, 311])
    );
    assert_eq!(chart, before);
    assert_eq!(
        p.options(Mode::Five).unwrap().family,
        ddi_danoni_export::extraction::Family::ScheduledCounter
    );
    assert!(p.options(Mode::Seven).is_err());
    assert!(p.options(Mode::NineA).is_err());
    assert!(p.options(Mode::NineB).is_err());
    assert!(
        EngineProfile::RekuConstantMenuEarly
            .options(Mode::Seven)
            .is_err()
    );
}
#[test]
fn variable_travel_rejects_wrong_clock_missing_or_ambiguous_schedules_and_cancels() {
    let profile = EngineProfile::RekuScheduledMenu;
    let origin = json!({"logicalFrame":145,"iteration":201});
    let good = scheduled_capture(json!({"frame_num":0,"judgeY":{"200":100,"100":150}}));
    for kind in [
        "clock",
        "missing",
        "duplicate",
        "snapshot",
        "agreement",
        "mode",
    ] {
        let mut c = good.clone();
        let mut chart = scheduled_chart();
        match kind {
            "clock" => {
                c = scheduled_capture(json!({"frame_num":1,"judgeY":{"200":100,"100":150}}));
            }
            "missing" => {
                c = scheduled_capture(json!({"frame_num":0,"judgeY":{"100":150}}));
            }
            "duplicate" => {
                chart["nativeSchedules"]["taps"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"frame":200,"lane":0,"count":1}));
            }
            "snapshot" => {
                chart["preparedIteration"] = json!(56);
            }
            "agreement" => {
                chart["scheduleAgreement"] = json!(false);
            }
            "mode" => {
                chart["mode"] = json!("9i");
            }
            _ => unreachable!(),
        }
        assert!(
            profile
                .motion_candidates(&c, &chart, &origin, &mut || Ok(()))
                .is_err(),
            "{kind}"
        );
    }
    let e = profile
        .motion_candidates(&good, &scheduled_chart(), &origin, &mut || {
            Err(Error {
                code: "host-cancel",
                message: "synthetic cancellation".into(),
            })
        })
        .unwrap_err();
    assert_eq!(e.code, "host-cancel");
}
#[test]
fn scheduled_menu_uses_its_observed_controls_and_rejects_unknown_settings() {
    let profile = EngineProfile::RekuScheduledMenu;
    let mut capture = menu(1);
    for item in capture["captures"].as_array_mut().unwrap() {
        let props = item["state"]["objects"][0]["properties"]
            .as_array_mut()
            .unwrap();
        props.push(json!({"name":string("motion"),"value":string("OFF")}));
        props.push(json!({"name":string("ber"),"value":string("Default")}));
    }
    let menu = profile.read_menu(&capture).unwrap();
    assert_eq!(menu.rows[1].mode, None);
    let controls = profile.selector_controls(&menu, 0).unwrap();
    assert_eq!(controls.events[0]["y"], 193);
    assert_eq!(controls.events[1]["y"], 69);
    let practice = controls.events.iter().find(|e| e["y"] == 169).unwrap();
    assert_eq!(practice["frame"], controls.start - 3);
    assert_eq!(controls.events.last().unwrap()["y"], 220);
    assert_eq!(controls.events.last().unwrap()["x"], 105);
    set_property(&mut capture, 0, "motion", string("ON"));
    assert!(profile.read_menu(&capture).is_err());
    set_property(&mut capture, 0, "motion", string("OFF"));
    set_property(&mut capture, 0, "ber", string("Unknown"));
    assert!(profile.read_menu(&capture).is_err());
}
