use ddi_danoni_export::{Result, assets::*, extraction::*, model::Mode};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn string(s: &str) -> Value {
    json!({"type":"string","text":"ignored display text","utf16":s.encode_utf16().collect::<Vec<_>>(),"truncated":false})
}
fn stored(v: &Value, objects: &mut Vec<Value>) -> Value {
    if let Value::String(s) = v {
        return string(s);
    }
    if !(v.is_object() || v.is_array()) || v["type"] == "undefined" {
        return v.clone();
    }
    let id = objects.len();
    objects.push(Value::Null);
    let mut properties = Vec::new();
    if let Some(array) = v.as_array() {
        properties.push(json!({"name":string("length"),"value":array.len()}));
        for (i, v) in array.iter().enumerate() {
            properties.push(json!({"name":string(&i.to_string()),"value":stored(v,objects)}));
        }
    } else {
        for (k, v) in v.as_object().unwrap() {
            properties.push(json!({"name":string(k),"value":stored(v,objects)}));
        }
    }
    objects[id] = json!({"id":id,"properties":properties,"truncated":false});
    json!({"ref":id})
}
fn capture(roots: Vec<Value>) -> Value {
    let items=roots.into_iter().enumerate().map(|(i,r)|{let mut objects=Vec::new();let reference=stored(&r,&mut objects);json!({"frame":10+i,"state":{"objects":objects,"roots":[{"level":0,"value":reference,"movieFrame":10+i}],"displayTree":[],"incomplete":false,"avm1Halted":false}})}).collect::<Vec<_>>();
    json!({"schema":"ddi-ruffle-probe/experimental-1","ruffleRevision":"89a7049965f3090e706265670df309a5128233ac","source":{"movieSha256":"0".repeat(64),"config":{"sourceUrl":"https://example.org/game.swf","resources":[]}},"swfHeaderFps":60,"avm1Halted":false,"audit":{"entries":[],"truncated":false,"runtimeErrors":0,"droppedEntries":0,"truncatedDetails":0},"captures":items})
}
fn options(family: Family) -> ProjectionOptions {
    ProjectionOptions {
        family,
        mode: Mode::Five,
        source_prefix: String::new(),
        source_suffix: None,
        pinned_selection: None,
        review_missing_decorative_sprites: false,
    }
}
fn five() -> Value {
    let mut root = json!({"controller":{"v":0,"w":0,"x":0,"y":0,"z":0}});
    for name in ["left", "down", "up", "right", "space"] {
        root[format!("{name}_data")] = json!("200,260");
        root["controller"][format!("{name}_timeline")] = json!(["200", "260"]);
    }
    root
}
fn project(v: &Value, o: &ProjectionOptions) -> Result<Projection> {
    project_capture(v, o, &mut || Ok(()))
}
#[test]
fn graph_uses_utf16_and_rejects_holes_duplicates_and_incomplete_values() {
    assert_eq!(text(&string("北風")).unwrap(), "北風");
    assert!(numeric(&Value::Bool(true)).is_err());
    assert!(numeric(&string("NaN")).is_err());
    let mut lone = string("x");
    lone["utf16"] = json!([0xd800]);
    assert!(text(&lone).is_err());
    lone = string("x");
    lone["truncated"] = json!(true);
    assert!(text(&lone).is_err());
    let mut c = capture(vec![json!([1, 2])]);
    let s = &mut c["captures"][0]["state"];
    let props = s["objects"][0]["properties"].as_array_mut().unwrap();
    props.retain(|p| text(&p["name"]).unwrap() != "1");
    let g = Graph::new(s).unwrap();
    assert!(g.array(&json!({"ref":0})).is_err());
    assert!(g.props(&json!({"ref":100})).is_err());
    let mut c = capture(vec![json!({"a":1})]);
    let s = &mut c["captures"][0]["state"];
    let p = s["objects"][0]["properties"][0].clone();
    s["objects"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(p);
    assert!(Graph::new(s).unwrap().root().is_err());
    s["incomplete"] = json!(true);
    assert!(Graph::new(s).is_err());
}
#[test]
fn repeated_child_banks_require_source_agreement_and_unconsumed_cursors() {
    let root = five();
    let c = capture(vec![root.clone(), root.clone()]);
    let o = options(Family::ChildTimeline5);
    let p = project(&c, &o).unwrap();
    assert_eq!(p.prepared_iteration, 10);
    assert_eq!(p.chart["lanes"].as_array().unwrap().len(), 5);
    let mut changed = root.clone();
    changed["left_data"] = json!("200,261");
    assert!(project(&capture(vec![root.clone(), changed]), &o).is_err());
    let mut consumed = root.clone();
    consumed["controller"]["v"] = json!(1);
    assert!(project(&capture(vec![root, consumed]), &o).is_err());
    let mut missing_speed = five();
    missing_speed["speed_data"] = json!("400,1.5");
    assert_eq!(
        project(&capture(vec![missing_speed.clone(), missing_speed]), &o)
            .unwrap_err()
            .code,
        "native-source-bank-missing"
    );
    let mut ambiguous = five();
    ambiguous["other"] = ambiguous["controller"].clone();
    assert!(project(&capture(vec![ambiguous.clone(), ambiguous]), &o).is_err());
    assert!(
        project_capture(&c, &o, &mut || Err(ddi_danoni_export::Error {
            code: "cancelled",
            message: "test".into()
        }))
        .is_err()
    );
}
#[test]
fn explicit_source_suffix_rejects_missing_and_ambiguous_fields() {
    let mut root = five();
    for name in ["left", "down", "up", "right", "space"] {
        root[format!("{name}_data2")] = root[format!("{name}_data")].clone();
        root.as_object_mut()
            .unwrap()
            .remove(&format!("{name}_data"));
    }
    let mut o = options(Family::ChildTimeline5);
    o.source_suffix = Some("2".into());
    assert!(project(&capture(vec![root.clone(), root.clone()]), &o).is_ok());
    root["left2_data"] = json!("200,260");
    assert!(project(&capture(vec![root.clone(), root]), &o).is_err());
}
fn constant_roots(delayed: bool) -> Vec<Value> {
    let mut taps = vec![json!([]); 7];
    let mut freezes = taps.clone();
    let indices = [0, 2, 4, 6, 3];
    for i in indices {
        taps[i] = json!([format!("{}", 200 + i * 10)]);
    }
    freezes[0] = json!(["300", "360"]);
    let raw = json!({"frame_num":0,"set_cnt":100,"set_spi":3,"dif_spi":1.5,"keyLabel":"5","diffi":"Normal","diffin":"","timeline":taps,"ftimeline":freezes,"mkArrow":{},"mkFArrow":{},"speed_dac":["400","1.5"],"keyCtrl":[37,65,40,32,38,68,39]});
    let mut prepared = raw.clone();
    let mut durations = vec![json!({}); 7];
    durations[0] = json!({"0":60});
    prepared["frzLength"] = json!(durations);
    for i in indices {
        prepared["timeline"][i] = json!([100 + i * 10]);
        prepared["mkArrow"][format!("{}", 100 + i * 10)] = json!([i]);
    }
    prepared["ftimeline"][0] = json!([200, 360]);
    prepared["mkFArrow"] = json!({"200":[0]});
    if !delayed {
        return vec![raw, prepared];
    }
    let mut raw = raw;
    raw.as_object_mut().unwrap().remove("frame_num");
    prepared.as_object_mut().unwrap().remove("frame_num");
    let mut final_state = prepared.clone();
    final_state.as_object_mut().unwrap().remove("timeline");
    final_state.as_object_mut().unwrap().remove("ftimeline");
    final_state["frame_num"] = json!(0);
    vec![raw, prepared, final_state]
}
#[test]
fn constant_sources_cross_check_mutated_heads_tails_durations_and_delayed_clock() {
    let o = options(Family::ScheduledConstant);
    let c = capture(constant_roots(true));
    let p = project(&c, &o).unwrap();
    assert_eq!(p.prepared_iteration, 12);
    assert_eq!(p.chart["compiledIteration"], 11);
    assert_eq!(p.chart["clockInitializationIteration"], 12);
    let mut roots = constant_roots(false);
    roots[1]["frzLength"][0]["0"] = json!(61);
    assert!(project(&capture(roots), &o).is_err());
    let mut roots = constant_roots(false);
    roots[1]["mkArrow"]["100"] = json!([2]);
    assert!(project(&capture(roots), &o).is_err());
}
#[test]
fn scheduled_counter_proves_every_head_from_spawn_plus_travel() {
    let mut root = json!({"frameNum":0,"diffin":"","keyLabel":"5","speed_data":"400,1.5","mkArrow":{},"mkFArrow":{"200":[0]},"judgeY":{"200":100},"frzLeft_data":"300,360"});
    for (i, name) in ["left", "down", "up", "right", "space"].iter().enumerate() {
        let index = [0, 2, 4, 6, 3][i];
        root[format!("{name}_data")] = json!(format!("{}", 210 + i * 10));
        root["mkArrow"][format!("{}", 110 + i * 10)] = json!([index]);
        root["judgeY"][format!("{}", 110 + i * 10)] = json!(100);
    }
    let o = options(Family::ScheduledCounter);
    let p = project(&capture(vec![root.clone()]), &o).unwrap();
    assert_eq!(p.chart["scheduleAgreement"], true);
    let relative = relative_chart(&p.chart, 145.0).unwrap();
    assert!(
        (relative["lanes"][0]["holdsSeconds"][0][1].as_f64().unwrap() - 215.0 / 60.0).abs() < 1e-12
    );
    root["judgeY"]["110"] = json!(101);
    assert!(project(&capture(vec![root]), &o).is_err());
}
#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl AssetSource for Memory {
    fn read(&self, _: &str, p: &str, max: usize) -> Result<Vec<u8>> {
        let v = self.0[p].clone();
        assert!(v.len() <= max);
        Ok(v)
    }
}
impl Memory {
    fn put(&mut self, name: &str, v: &Value) -> Binding {
        let bytes = serde_json::to_vec(v).unwrap();
        let h = sha256(&bytes);
        self.0.insert(name.into(), bytes);
        Binding {
            namespace: "test".into(),
            path: name.into(),
            sha256: h,
        }
    }
}
#[test]
fn successful_parent_run_and_exact_config_are_required_before_projection() {
    let mut store = Memory::default();
    let root = five();
    let mut c = capture(vec![root.clone(), root]);
    let movie = store.put(
        "movie.swf",
        &json!("Original synthetic identity bytes; never executed"),
    );
    c["source"]["movieSha256"] = json!(movie.sha256);
    let config = store.put("config.json", &c["source"]["config"]);
    let run=store.put("run.json",&json!({"schema":"ddi-ruffle-run/experimental-1","exitCode":0,"termination":null,"inputsUnchanged":true,"configSha256":config.sha256}));
    let snapshot = store.put("snapshot.json", &c);
    let mut request = ProjectionRequest {
        capture: CaptureBindings {
            snapshot,
            run,
            config,
        },
        movie,
        options: options(Family::ChildTimeline5),
        optional_color: None,
    };
    assert!(prepare_projection(&request, &store, &mut || Ok(())).is_ok());
    request.capture.run=store.put("run.json",&json!({"schema":"ddi-ruffle-run/experimental-1","exitCode":0,"termination":"wall-limit","inputsUnchanged":true,"configSha256":request.capture.config.sha256}));
    assert!(prepare_projection(&request, &store, &mut || Ok(())).is_err());
}
#[test]
fn optional_color_exception_is_exact_and_original_audit_is_preserved() {
    let mut c = capture(vec![five(), five()]);
    let error = "message=Asynchronous error occurred: Could not fetch: \"Invalid URL iro.txt: no exact GET match in immutable resource cache\" ";
    c["audit"]["runtimeErrors"] = json!(1);
    c["audit"]["entries"] = json!([{"kind":"fetch-denied","detail":"iro.txt","iteration":11},{"kind":"runtime-error","detail":error,"iteration":11}]);
    let review = json!({"schema":"ddi-native-optional-color-review/1","sourceSwfSha256":"0".repeat(64),"missingResources":[{"url":"https://example.org/iro.txt","status":404,"sha256":"1".repeat(64),"retrievedAt":"2026-10-13"}]});
    let before = c.clone();
    let (clean, retained) = review_optional_colors(&c, &review).unwrap();
    assert_eq!(c, before);
    assert_eq!(retained.len(), 2);
    assert_eq!(clean["audit"]["runtimeErrors"], 0);
    c["audit"]["entries"][1]["detail"] = json!("unrelated error");
    assert!(review_optional_colors(&c, &review).is_err());
}

fn dac_root(fixed: bool) -> Value {
    let mut root = json!({"speed":"1.25","set_spi":2.5,"olds":2.5,"diffi":"Normal","diffin":"","speed_data":"400,1.5,//inactive","speed_dac":["400","1.5","//inactive"],"stop_data":"500,560","controller":{"cunt":0,"sp":0,"stopcunt":0,"stop_dac":["500","560"]}});
    for (name, clip) in [
        ("left", "le"),
        ("down", "do"),
        ("up", "up"),
        ("right", "ri"),
        ("space", "oni"),
    ] {
        root[format!("{name}_data")] = json!("200,260,");
        root["controller"][format!("{clip}_dac")] = json!(["200", "260", ""]);
        root[format!("{clip}s")] = json!(0);
        root[format!("f{clip}s")] = json!(0);
        root[format!("f{clip}_dac")] = json!([]);
    }
    root["fle_dac"] = json!(["300", "360"]);
    root["frzLeft_data"] = json!("300,360");
    if fixed {
        for key in ["LE", "DO", "UP", "RI", "SH"] {
            root[format!("key_{key}")] = json!(false);
        }
        root["oni_spi"] = json!(2.5);
    } else {
        root["keyLabel"] = json!("5");
        root["keyCtrl"] = json!([37, 65, 40, 32, 38, 68, 39]);
    }
    root
}
#[test]
fn dac_families_preserve_additive_and_multiplicative_speed_and_inactive_tail() {
    for (fixed, family, ratio) in [(false, Family::Dac5, 1.4), (true, Family::DacFixed5, 1.5)] {
        let root = dac_root(fixed);
        let p = project(&capture(vec![root.clone(), root.clone()]), &options(family)).unwrap();
        assert!((p.chart["speed"][0]["multiplier"].as_f64().unwrap() - ratio).abs() < 1e-12);
        assert_eq!(p.chart["inactiveSpeedTail"].as_array().unwrap().len(), 1);
        let mut overflowing = root.clone();
        overflowing["set_spi"] = json!(if fixed { 1e308 } else { 1e-310 });
        overflowing["speed"] = json!(if fixed { 5e307 } else { 5e-311 });
        overflowing["olds"] = overflowing["set_spi"].clone();
        if fixed {
            overflowing["oni_spi"] = json!(1e308);
        }
        overflowing["speed_data"] = json!("400,10");
        overflowing["speed_dac"] = json!(["400", "10"]);
        assert!(
            project(
                &capture(vec![overflowing.clone(), overflowing]),
                &options(family)
            )
            .is_err()
        );
        let mut null = root.clone();
        null["fle_dac"] = Value::Null;
        assert!(project(&capture(vec![null.clone(), null]), &options(family)).is_err());
        let mut unmatched = root;
        unmatched["speed_data"] = json!("400,1.5,2,//inactive");
        unmatched["speed_dac"] = json!(["400", "1.5", "2", "//inactive"]);
        assert!(
            project(
                &capture(vec![unmatched.clone(), unmatched]),
                &options(family)
            )
            .is_err()
        );
    }
}
#[test]
fn parafla_uses_parsed_pixel_speeds_and_retains_key_zero_inactive_schedules() {
    let mut root = json!({"frameNum":0,"keyLabel":"5","diffin":"1","DOS_DIVIDE":true,"setSpd":3,"difSpd":1.5,"arrowSpd":[0,3,400,6],"speed_change":"400,9999","keyCtrl":[37,0,40,32,38,0,39],"mkArrow":{"190":[1,5]},"mkFArrow":{},"judgeY":{"190":100}});
    for (i, name) in ["left", "down", "up", "right", "space"].iter().enumerate() {
        let index = [0, 2, 4, 6, 3][i];
        root[format!("{name}_data")] = json!(format!("{}", 210 + i * 10));
        root["mkArrow"][format!("{}", 110 + i * 10)] = json!([index]);
        root["judgeY"][format!("{}", 110 + i * 10)] = json!(100);
    }
    let o = options(Family::ParaflaCounter);
    let p = project(&capture(vec![root.clone()]), &o).unwrap();
    assert_eq!(p.chart["speed"][0]["multiplier"], json!(2.0));
    assert_eq!(
        p.chart["inactiveSchedules"]["taps"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(p.chart["sourceSelection"]["loadedFieldSuffix"], "");
    root["keyCtrl"][1] = json!(65);
    assert!(project(&capture(vec![root]), &o).is_err());
}
#[test]
fn audio_origin_uses_exact_counter_snapshot_and_keeps_stream_boundary() {
    let mut c = capture(vec![json!({"frameNum":144}), json!({"frameNum":145})]);
    c["audio"] = json!({"streamStarts":[{"iteration":11,"compression":"Mp3","outputSampleFrame":44100},{"iteration":20,"compression":"Mp3","outputSampleFrame":88200}]});
    let origin = movie_audio_origin(&c, Some("frameNum"), 0).unwrap();
    assert_eq!(origin["logicalFrame"], json!(145.0));
    assert_eq!(origin["nextStreamOutputSampleFrame"], 88200);
    assert!(movie_audio_origin(&c, Some("frameNum"), 1).is_err());
    c["audio"]["streamStarts"][0]["compression"] = json!("Pcm");
    assert!(movie_audio_origin(&c, Some("frameNum"), 0).is_err());
}

#[test]
fn movement_and_audio_runs_must_share_the_same_counter_origin() {
    let origin = json!({"logicalFrame":145,"iteration":100,"clock":"frame_num"});
    let mut motion = json!({"clockField":"frame_num","samples":[{"movieFrame":146,"iteration":101},{"movieFrame":147,"iteration":102},{"movieFrame":148,"iteration":103}]});
    validate_clock_agreement(&origin, &motion).unwrap();
    motion["samples"][2]["movieFrame"] = json!(149);
    assert_eq!(
        validate_clock_agreement(&origin, &motion).unwrap_err().code,
        "native-clock-mismatch"
    );
}
