//! Original synthetic movies' stored states, not acquired work fixtures.
use ddi_danoni_export::{extraction::*, model::Mode};
use serde_json::{Value, json};
fn string(s: &str) -> Value {
    json!({"type":"string","utf16":s.encode_utf16().collect::<Vec<_>>(),"truncated":false})
}
fn store(v: &Value, objects: &mut Vec<Value>) -> Value {
    if let Some(s) = v.as_str() {
        return string(s);
    }
    if !v.is_array() && !v.is_object() || v["type"] == "undefined" {
        return v.clone();
    }
    let id = objects.len();
    objects.push(Value::Null);
    let mut p = vec![];
    if let Some(a) = v.as_array() {
        p.push(json!({"name":string("length"),"value":a.len()}));
        for (i, v) in a.iter().enumerate() {
            p.push(json!({"name":string(&i.to_string()),"value":store(v,objects)}));
        }
    } else {
        for (k, v) in v.as_object().unwrap() {
            p.push(json!({"name":string(k),"value":store(v,objects)}));
        }
    }
    objects[id] = json!({"id":id,"truncated":false,"properties":p});
    json!({"ref":id})
}
fn item(frame: u64, root_frame: u64, root: Value) -> Value {
    let mut objects = vec![];
    let value = store(&root, &mut objects);
    json!({"frame":frame,"state":{"incomplete":false,"avm1Halted":false,"objects":objects,"roots":[{"level":0,"value":value,"movieFrame":root_frame}],"displayTree":[{"id":0,"parent":null,"kind":"movie-clip","name":string(""),"visible":true,"movieFrame":root_frame,"object":value}]}})
}
fn capture(hash: &str, items: Vec<Value>, start: Value) -> Value {
    json!({"schema":"ddi-ruffle-probe/experimental-1","source":{"movieSha256":hash,"config":{"resources":[]}},"build":{"synthetic":true},"swfHeaderFps":60,"avm1Halted":false,"audit":{"entries":[],"truncated":false,"runtimeErrors":0,"droppedEntries":0,"truncatedDetails":0},"captures":items,"audio":{"streamStarts":[start]}})
}
fn options(family: Family) -> ProjectionOptions {
    ProjectionOptions {
        family,
        mode: Mode::Five,
        source_prefix: String::new(),
        source_suffix: None,
        pinned_selection: (family == Family::Kouhaku2004).then_some(PinnedSelection::Tougenkyo),
        review_missing_decorative_sprites: false,
    }
}
fn project(c: &Value, o: &ProjectionOptions) -> ddi_danoni_export::Result<Projection> {
    project_capture(c, o, &mut || Ok(()))
}
fn root_prop<'a>(c: &'a mut Value, frame: usize, key: &str) -> &'a mut Value {
    c["captures"][frame]["state"]["objects"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| text(&p["name"]).unwrap() == key)
        .unwrap()
        .get_mut("value")
        .unwrap()
}
fn higan() -> Value {
    let mut split = json!({"keyLabel":"5","difficulty":1,"diffin":"","motion":"OFF","correction":0,"set_cnt":100,"startFrame":0,"frame_num":0,"set_spi":3,"dif_spi":"1.5","diffi":"Normal","musicTitle":"Synthetic","tuning":"Synthetic author","keyCtrl":[37,0,40,32,38,0,39]});
    split["timeline"] = json!((0..7).map(|_| vec!["200"]).collect::<Vec<_>>());
    split["ftimeline"] = json!((0..7).map(|_| vec!["220", "250"]).collect::<Vec<_>>());
    let mut prepared = split.clone();
    prepared["timeline"] = json!((0..7).map(|_| vec![100]).collect::<Vec<_>>());
    prepared["ftimeline"] = json!((0..7).map(|_| vec![120, 250]).collect::<Vec<_>>());
    prepared["frzLength"] = json!((0..7).map(|_| json!({"0":30})).collect::<Vec<_>>());
    prepared["mkArrow"] = json!({"100":[0,2,4,6,3]});
    prepared["mkFArrow"] = json!({"120":[0,2,4,6,3]});
    let mut clean = prepared.clone();
    clean.as_object_mut().unwrap().remove("timeline");
    clean["speed_dac"] = json!([500, 1.5]);
    clean["iro_dac"] = json!([300, "0x00ff00", "al", 400, "0xff0000", "oni"]);
    clean["stop_dac"] = json!([""]);
    let mut audio = clean.clone();
    audio["frame_num"] = json!(145);
    let mut ai = item(201, 149, audio);
    ai["state"]["displayTree"].as_array_mut().unwrap().push(json!({"id":1,"parent":0,"kind":"movie-clip","name":string("music_act"),"visible":true,"movieFrame":2,"object":null}));
    capture(
        "6ce53b126e90b25a9f307595bf226ed4e6ce8f70c74d816a1d372c7f20de6e85",
        vec![
            item(52, 4, split),
            item(53, 4, prepared),
            item(55, 4, clean),
            ai,
        ],
        json!({"iteration":201,"compression":"Mp3","latencySeek":0,"outputSampleFrame":147000}),
    )
}
fn kouhaku() -> Value {
    let mut root = json!({"artist":"公","difficulty":"★★★"});
    for name in ["left", "down", "up", "right", "space"] {
        root[format!("{name}_data")] = json!("212,272");
    }
    let mut caps = vec![item(90, 4, root.clone())];
    for frame in [100, 101, 102] {
        let mut i = item(frame, 6, root.clone());
        let s = &mut i["state"];
        let mut controller = json!({"Cnt":1});
        for name in ["left", "down", "up", "right", "space"] {
            controller[format!("{name}_timeline")] = json!([212, 272]);
        }
        let reference = store(&controller, s["objects"].as_array_mut().unwrap());
        s["displayTree"].as_array_mut().unwrap().extend([
            json!({"id":1,"parent":0,"name":string("song"),"movieFrame":frame-98,"object":null}),
            json!({"id":2,"parent":1,"name":string("controller"),"object":reference}),
        ]);
        s["globalScopes"] = json!([{"caseSensitive":false,"properties":([("kyoku",json!("桃源郷")),("Reverse",json!(false)),("SpeedUp",json!(2)),("SpeedY",json!(188)),("point",json!(0))].iter().map(|(k,v)|json!({"name":string(k),"value":if let Some(s)=v.as_str(){string(s)}else{v.clone()},"accessor":false})).collect::<Vec<_>>())}]);
        caps.push(i);
    }
    capture(
        "e14e472911b6603ddcc58bd40049d0b05cfa986a2240209dd848a516cf40fed2",
        caps,
        json!({"iteration":100,"compression":"Mp3","sampleRate":22050,"stereo":false,"latencySeek":1633,"outputSampleFrame":72765}),
    )
}
#[test]
fn higan_checks_split_holds_durations_schedule_and_keeps_effect_units() {
    let c = higan();
    let o = options(Family::HiganPilot);
    let p = project(&c, &o).unwrap();
    assert_eq!(p.prepared_iteration, 53);
    assert_eq!(
        p.chart["lanes"][0]["nativeHoldFrames"],
        json!([[220.0, 250.0]])
    );
    let r = relative_chart(&p.chart, 145.0).unwrap();
    assert_eq!(r["lanes"][0]["tapsSeconds"], json!([55.0 / 60.0]));
    assert_eq!(r["colors"].as_array().unwrap().len(), 6);
    assert_eq!(r["colors"][0]["seconds"], 55.0 / 60.0);
    assert_eq!(r["speed"][0]["seconds"], 355.0 / 60.0);
    let mut bad = c.clone();
    *root_prop(&mut bad, 1, "set_cnt") = json!(101);
    assert!(project(&bad, &o).is_err());
    let mut bad = c.clone();
    let reference = root_prop(&mut bad, 2, "mkArrow")["ref"].as_u64().unwrap() as usize;
    bad["captures"][2]["state"]["objects"][reference]["properties"]
        .as_array_mut()
        .unwrap()
        .clear();
    assert!(project(&bad, &o).is_err());
    let mut bad = c.clone();
    bad["audio"]["streamStarts"][0]["latencySeek"] = json!(1);
    assert!(project(&bad, &o).is_err());
    let mut bad = c.clone();
    bad["captures"][1]["frame"] = json!(51);
    assert!(project(&bad, &o).is_err());
    let mut o = o;
    o.pinned_selection = Some(PinnedSelection::Run);
    assert!(project(&c, &o).is_err());
}
#[test]
fn kouhaku_checks_selection_stars_child_clock_and_compiled_data() {
    let c = kouhaku();
    let o = options(Family::Kouhaku2004);
    let p = project(&c, &o).unwrap();
    assert_eq!(p.prepared_iteration, 100);
    assert_eq!(p.chart["difficultyStars"], 3);
    assert_eq!(p.chart["pinnedClockPath"], json!(["", "song"]));
    assert_eq!(
        relative_chart(&p.chart, 2.0).unwrap()["lanes"][0]["tapsSeconds"],
        json!([3.5, 4.5])
    );
    let mut bad = c.clone();
    *root_prop(&mut bad, 0, "left_data") = string("212,273");
    assert!(project(&bad, &o).is_err());
    let mut bad = c.clone();
    *root_prop(&mut bad, 1, "difficulty") = string("★★★★");
    assert!(project(&bad, &o).is_err());
    let mut bad = c.clone();
    bad["captures"][2]["state"]["displayTree"][1]["movieFrame"] = json!(4);
    assert!(project(&bad, &o).is_err());
    let mut bad = c.clone();
    bad["captures"][1]["state"]["displayTree"][1]["parent"] = json!(1);
    assert!(project(&bad, &o).is_err());
    let mut bad = c.clone();
    bad["captures"][1]["state"]["globalScopes"][0]["properties"][0]["accessor"] = json!(true);
    assert!(project(&bad, &o).is_err());
    let mut bad = c.clone();
    bad["source"]["movieSha256"] = json!("0".repeat(64));
    assert!(project(&bad, &o).is_err());
    let mut o = o;
    o.pinned_selection = Some(PinnedSelection::TougenkyoHard);
    assert!(project(&c, &o).is_err());
}
#[test]
fn child_clock_paths_must_agree_and_cancellation_is_observed() {
    let origin = json!({"clock":"pinned-child-timeline","clockPath":["","song"],"logicalFrame":2,"iteration":100});
    let mut motion = json!({"clockField":"pinned-child-timeline","clockPath":["","song"],"samples":[{"movieFrame":3,"iteration":101},{"movieFrame":4,"iteration":102},{"movieFrame":5,"iteration":103}]});
    validate_clock_agreement(&origin, &motion).unwrap();
    motion["clockPath"] = json!(["", "other"]);
    assert!(validate_clock_agreement(&origin, &motion).is_err());
    let mut calls = 0;
    let e = project_capture(&kouhaku(), &options(Family::Kouhaku2004), &mut || {
        calls += 1;
        if calls == 3 {
            Err(ddi_danoni_export::Error {
                code: "cancelled",
                message: "test".into(),
            })
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(e.code, "cancelled");
}
