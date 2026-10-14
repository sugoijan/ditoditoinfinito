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
