#![cfg(feature = "native")]
//! Optional real-worker gate. It reuses local cache only and never fetches works.
use ddi_danoni_export::{
    assets::*, native::DirectorySource, network::NetworkCancel, url_prepare::*, worker_build,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn verify(output: &Path, receipt: &Value) -> usize {
    let p = output.join(receipt["completed"]["prepared"]["path"].as_str().unwrap());
    assert_eq!(
        sha256(&fs::read(&p).unwrap()),
        receipt["completed"]["prepared"]["sha256"]
    );
    let w: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    let root = p.parent().unwrap();
    for f in w["artifacts"].as_array().unwrap() {
        let bytes = fs::read(root.join(f["path"].as_str().unwrap())).unwrap();
        assert_eq!(sha256(&bytes), f["sha256"]);
        assert_eq!(bytes.len() as u64, f["bytes"].as_u64().unwrap());
    }
    let mut endpoints = 0;
    for s in w["songs"].as_array().unwrap() {
        let text = fs::read_to_string(root.join(s["simfile"].as_str().unwrap())).unwrap();
        let parsed = ddi_chart::formats::sm::parse_ssc(&text).unwrap();
        assert_eq!(parsed.charts.len(), s["charts"].as_array().unwrap().len());
        for (c, r) in parsed.charts.iter().zip(s["charts"].as_array().unwrap()) {
            let timing = c.timing(&parsed);
            let mut notes = BTreeMap::new();
            for n in &c.notes {
                notes.insert((n.lane as usize, n.tick.0), timing.seconds_at(n.tick));
                if let ddi_chart::NoteKind::HoldHead { end } = n.kind {
                    notes.insert((n.lane as usize, end.0), timing.seconds_at(end));
                }
            }
            assert_eq!(notes.len(), r["endpoints"].as_array().unwrap().len());
            let padding = r["paddingTicks"].as_i64().unwrap();
            for e in r["endpoints"].as_array().unwrap() {
                let t = notes[&(
                    e["lane"].as_u64().unwrap() as usize,
                    e["tick"].as_i64().unwrap() + padding,
                )];
                assert!((t - e["sourceSeconds"].as_f64().unwrap()).abs() <= 0.0090001);
                endpoints += 1;
            }
            // Sample interval velocities away from every quantized speed boundary.
            let mut boundaries = vec![0.0, 600.0];
            boundaries.extend(timing.scrolls.iter().map(|s| timing.seconds_at(s.tick)));
            boundaries.sort_by(f64::total_cmp);
            boundaries.dedup();
            let base = r["nativeMotion"]["baseArrowHeightsPerSecond"]
                .as_f64()
                .unwrap();
            for window in boundaries.windows(2).filter(|w| w[1] - w[0] > 0.1) {
                let time = (window[0] + window[1]) * 0.5;
                let source_chart = w["sourceManifest"]["charts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["id"] == r["id"])
                    .unwrap();
                let multiplier = source_chart["speed"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .rfind(|s| s["seconds"].as_f64().unwrap() <= time)
                    .map_or(1.0, |s| s["multiplier"].as_f64().unwrap());
                let factor = w["request"]["options"]["scrollFactor"].as_f64().unwrap();
                for x in [1.0, 2.0, 3.0] {
                    let opts = ddi_engine::scroll::ScrollOptions {
                        speed: ddi_engine::scroll::SpeedMod::XMod(x),
                        ..Default::default()
                    };
                    let target = timing.displayed_beat(timing.beat_at(time));
                    let y = |t| {
                        ddi_engine::scroll::ScrollState::at(opts, timing, t)
                            .y_displayed(target, time) as f64
                    };
                    let measured = (y(time) - y(time + 0.01)) / 0.01;
                    assert!((measured - base * multiplier * factor * x).abs() < 0.002);
                    assert!(base > 0.0);
                }
            }
        }
    }
    endpoints
}
#[test]
fn local_url_preparation_is_self_contained_and_late_cancel_cannot_publish() {
    let (Some(job), Some(bundle), Some(destination)) = (
        std::env::var_os("DDI_DANONI_PREPARED_URL_JOB"),
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
        std::env::var_os("DDI_DANONI_PREPARED_URL_OUTPUT"),
    ) else {
        return;
    };
    let path = PathBuf::from(job);
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let request: Request = serde_json::from_value(doc["request"].clone()).unwrap();
    assert!(
        request.source.fetch.offline,
        "Must never fetch a real work in this gate"
    );
    let parent = path.parent().unwrap();
    let cache = PathBuf::from(doc["cache"].as_str().unwrap());
    let cache = if cache.is_absolute() {
        cache
    } else {
        parent.join(cache)
    };
    let roots: BTreeMap<String, PathBuf> = serde_json::from_value(doc["roots"].clone()).unwrap();
    let host = DirectorySource::new(
        roots
            .into_iter()
            .map(|(k, p)| (k, if p.is_absolute() { p } else { parent.join(p) }))
            .collect(),
    )
    .unwrap();
    let worker = worker_build::load_bundle(&PathBuf::from(bundle), &mut || Ok(())).unwrap();
    let output = PathBuf::from(destination);
    fs::create_dir(&output).unwrap();
    let mut receipts = Vec::new();
    for cancel_at_ready in [false, true] {
        let dir = output.join(if cancel_at_ready {
            "late-cancel"
        } else {
            "success"
        });
        let cancel = NetworkCancel::default();
        let out = execute(
            &request,
            &cache,
            &worker,
            &host,
            &dir,
            &cancel,
            &mut |_| panic!("Offline gate requested live site approval"),
            &mut |p| {
                if p.stage == "ready" {
                    assert!(dir.join(".manifest.json.tmp").is_file());
                    assert!(dir.join("prepared/manifest.json").is_file());
                    if cancel_at_ready {
                        cancel.cancel();
                    }
                }
                Ok(())
            },
        )
        .unwrap();
        if cancel_at_ready {
            assert!(!out.ready);
            assert!(out.manifest["completed"].is_null());
            assert_eq!(out.failure.unwrap().code, "request-cancelled");
            assert!(!dir.join("manifest.json").exists());
            assert!(!dir.join(".manifest.json.tmp").exists());
            assert!(dir.join("failure.json").is_file());
            receipts.push(json!({"cancelled":true,"childPrepared":true,"parentReady":false}));
        } else {
            assert!(out.ready, "{:?}", out.failure);
            let count = verify(&dir, &out.manifest);
            receipts.push(json!({"cancelled":false,"endpoints":count,"parent":out.manifest}));
        }
    }
    fs::write(
        output.join("validation.json"),
        serde_json::to_vec_pretty(
            &json!({"schema":"ddi-danoni-url-preparation-validation/1","runs":receipts}),
        )
        .unwrap(),
    )
    .unwrap();
}

struct Replay<'a> {
    files: DirectorySource,
    manifest: &'a Value,
}
impl AssetSource for Replay<'_> {
    fn read(&self, ns: &str, path: &str, maximum: usize) -> ddi_danoni_export::Result<Vec<u8>> {
        let key = format!("{ns}:{path}");
        let relative =
            self.manifest["inputArtifacts"][&key]
                .as_str()
                .ok_or(ddi_danoni_export::Error {
                    code: "missing-replay-input",
                    message: key,
                })?;
        self.files.read("prepared", relative, maximum)
    }
}
#[test]
fn prepared_result_replays_without_source_tree_cache_font_or_worker() {
    let Some(root) = std::env::var_os("DDI_DANONI_PREPARED_REPLAY") else {
        return;
    };
    let root = PathBuf::from(root);
    let outer: Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    assert!(outer["ready"].as_bool().unwrap());
    let endpoints = verify(&root, &outer);
    let dir = root.join("prepared");
    let manifest: Value =
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    let replay = Replay {
        files: DirectorySource::new(BTreeMap::from([("prepared".into(), dir.clone())])).unwrap(),
        manifest: &manifest,
    };
    let request = serde_json::from_value(manifest["request"].clone()).unwrap();
    let rebuilt =
        ddi_danoni_export::work::prepare_work(&request, &replay, &mut |_| Ok(())).unwrap();
    assert_eq!(rebuilt.manifest["songs"], manifest["songs"]);
    for artifact in rebuilt.files {
        assert_eq!(*artifact.bytes, fs::read(dir.join(artifact.path)).unwrap());
    }
    println!(
        "{}",
        json!({"schema":"ddi-danoni-prepared-replay/1","output":root,"endpoints":endpoints,
        "independentOfOriginalSourceRoots":true,"passed":true})
    );
}
