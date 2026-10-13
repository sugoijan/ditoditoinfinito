#![cfg(feature = "native")]
use ddi_danoni_export::{Error, assets::*, orchestration::*, supervisor::WorkerSpec};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Memory(BTreeMap<String, Vec<u8>>);
impl AssetSource for Memory {
    fn read(&self, namespace: &str, path: &str, max: usize) -> ddi_danoni_export::Result<Vec<u8>> {
        assert_eq!(namespace, "source");
        relative_path(path)?;
        let b = self.0.get(path).ok_or_else(|| Error {
            code: "missing-test-asset",
            message: path.into(),
        })?;
        if b.len() > max {
            return Err(Error {
                code: "asset-limit",
                message: path.into(),
            });
        }
        Ok(b.clone())
    }
}
fn binding(path: &str, bytes: &[u8]) -> Binding {
    Binding {
        namespace: "source".into(),
        path: path.into(),
        sha256: sha256(bytes),
    }
}
fn fixture() -> (NativePlan, Memory) {
    let mut movie = b"FWS\x06".to_vec();
    movie.extend(18u32.to_le_bytes());
    movie.extend([0x08, 0, 0, 60, 1, 0, 0x40, 0, 0, 0]);
    let provenance = serde_json::to_vec(&json!({"sourceSwfSha256":sha256(&movie)})).unwrap();
    let text = b"original fixture data".to_vec();
    let config=serde_json::to_vec(&json!({"movie":"movie.swf","sourceUrl":"https://example.test/song.swf?name=num301","pageUrl":"https://example.test/work.html?name=num301","flashVars":{"name":"num301"},"events":[{"type":"click","frame":2,"x":70,"y":212}],"captureFrames":[1,2],"resources":[{"url":"https://example.test/chart.txt?name=num301","path":"chart.txt","sha256":sha256(&text),"httpCharset":"Shift_JIS"}]})).unwrap();
    let row = json!({"id":"normal","label":"Normal","options":{"family":"child-timeline-5","mode":"5","sourceSuffix":null,"pinnedSelection":null},"config":binding("inputs/config.json",&config),"projection":{"captureFrames":[1,2],"mixAudio":true,"render":false},"motion":{"captureFrames":[1,2],"mixAudio":true,"render":false},"audio":{"captureFrames":[1,2],"mixAudio":true,"render":false},"motionSelection":{"note":"leftMC2","receptor":"left_step","lane":0,"iterations":[1,2],"targetY":null,"geometryReference":null},"streamStartIndex":0,"optionalColor":null});
    let plan:NativePlan=serde_json::from_value(json!({"sourceManifest":binding("manifest.json",&provenance),"movie":binding("inputs/movie.swf",&movie),"metadata":{},"charts":[row]})).unwrap();
    (
        plan,
        Memory(BTreeMap::from([
            ("manifest.json".into(), provenance),
            ("inputs/movie.swf".into(), movie),
            ("inputs/chart.txt".into(), text),
            ("inputs/config.json".into(), config),
        ])),
    )
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ddi-orchestration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn roles_share_controls_and_identical_runs_are_deduplicated() {
    let (mut plan, source) = fixture();
    let p = prepare_plan(&plan, &source, &mut || Ok(())).unwrap();
    assert_eq!(p.captures.len(), 1);
    let original: Value = serde_json::from_slice(&source.0["inputs/config.json"]).unwrap();
    let c: Value = serde_json::from_slice(
        &p.files
            .iter()
            .find(|f| f.path == p.captures[0].request.config.path)
            .unwrap()
            .bytes,
    )
    .unwrap();
    for key in ["events", "sourceUrl", "pageUrl", "flashVars"] {
        assert_eq!(c[key], original[key]);
    }
    assert_eq!(c["resources"][0]["url"], original["resources"][0]["url"]);
    assert_eq!(c["resources"][0]["httpCharset"], "Shift_JIS");
    assert_eq!(c["movie"], format!("blobs/{}", plan.movie.sha256));
    assert_eq!(
        *p.files
            .iter()
            .find(|f| f.path == "inputs/original/normal.json")
            .unwrap()
            .bytes,
        source.0["inputs/config.json"]
    );
    plan.charts[0].motion.capture_frames = vec![1, 2, 3];
    plan.charts[0].motion.mix_audio = false;
    let p = prepare_plan(&plan, &source, &mut || Ok(())).unwrap();
    assert_eq!(p.captures.len(), 2);
    let mut controls = vec![];
    for cap in p.captures {
        let mut c: Value = serde_json::from_slice(
            &p.files
                .iter()
                .find(|f| f.path == cap.request.config.path)
                .unwrap()
                .bytes,
        )
        .unwrap();
        for key in ["captureFrames", "render", "mixAudio", "avmExecutionMs"] {
            c.as_object_mut().unwrap().remove(key);
        }
        controls.push(c);
    }
    assert_eq!(controls[0], controls[1]);
}
#[test]
fn staged_replay_reproduces_capture_configs_without_original_roots() {
    let (plan, source) = fixture();
    let first = prepare_plan(&plan, &source, &mut || Ok(())).unwrap();
    struct Archive(BTreeMap<String, Vec<u8>>);
    impl AssetSource for Archive {
        fn read(
            &self,
            namespace: &str,
            path: &str,
            max: usize,
        ) -> ddi_danoni_export::Result<Vec<u8>> {
            assert_eq!(namespace, "plan");
            relative_path(path)?;
            let b = &self.0[path];
            assert!(b.len() <= max);
            Ok(b.clone())
        }
    }
    let archive = Archive(
        first
            .files
            .iter()
            .map(|f| (f.path.clone(), f.bytes.as_ref().clone()))
            .collect(),
    );
    let job: PlanJob = serde_json::from_slice(&archive.0["replay-job.json"]).unwrap();
    let replay = prepare_plan(&job.plan, &archive, &mut || Ok(())).unwrap();
    assert_eq!(
        first
            .captures
            .iter()
            .map(|c| (&c.id, &c.request.config.sha256))
            .collect::<Vec<_>>(),
        replay
            .captures
            .iter()
            .map(|c| (&c.id, &c.request.config.sha256))
            .collect::<Vec<_>>()
    );
}
#[test]
fn bad_source_control_resource_or_observation_fails_planning() {
    let (plan, source) = fixture();
    for kind in [
        "hash", "movie", "resource", "frames", "audio", "motion", "identity", "limits",
    ] {
        let mut p = plan.clone();
        let mut s = Memory(source.0.clone());
        match kind {
            "hash" => p.movie.sha256 = "0".repeat(64),
            "movie" => {
                s.0.insert("inputs/movie.swf".into(), b"changed".to_vec());
            }
            "resource" => {
                s.0.insert("inputs/chart.txt".into(), b"changed".to_vec());
            }
            "frames" => p.charts[0].projection.capture_frames = vec![1201],
            "audio" => p.charts[0].audio.mix_audio = false,
            "motion" => p.charts[0].motion_selection.iterations = vec![700],
            "identity" => {
                let mut c = p.charts[0].clone();
                c.id = "NORMAL".into();
                p.charts.push(c);
            }
            "limits" => p.limits.wall_ms = 60001,
            _ => unreachable!(),
        }
        assert!(prepare_plan(&p, &s, &mut || Ok(())).is_err(), "{kind}");
    }
    let mut observation = serde_json::to_value(&plan.charts[0].motion).unwrap();
    observation["events"] = json!([]);
    assert!(serde_json::from_value::<Observation>(observation).is_err());
}
#[test]
fn default_resources_are_explicit_and_capture_counts_are_bounded() {
    let (mut plan, mut source) = fixture();
    let mut c: Value = serde_json::from_slice(&source.0["inputs/config.json"]).unwrap();
    c.as_object_mut().unwrap().remove("resources");
    let b = serde_json::to_vec(&c).unwrap();
    plan.charts[0].config = binding("inputs/config.json", &b);
    source.0.insert("inputs/config.json".into(), b);
    let prepared = prepare_plan(&plan, &source, &mut || Ok(())).unwrap();
    let cap = &prepared.captures[0];
    let c: Value = serde_json::from_slice(
        &prepared
            .files
            .iter()
            .find(|f| f.path == cap.request.config.path)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(c["resources"], json!([]));
    let original = plan.charts[0].clone();
    plan.charts.clear();
    for i in 0..16u64 {
        let mut c = original.clone();
        c.id = format!("chart-{i}");
        c.projection.capture_frames = vec![i * 10 + 1, i * 10 + 2];
        c.motion.capture_frames = vec![i * 10 + 3, i * 10 + 4];
        c.motion_selection.iterations = c.motion.capture_frames.clone();
        c.audio.capture_frames = vec![i * 10 + 5, i * 10 + 6];
        plan.charts.push(c);
    }
    assert!(
        prepare_plan(&plan, &source, &mut || Ok(()))
            .err()
            .unwrap()
            .message
            .contains("32 distinct captures")
    );
}
#[test]
fn optional_response_relocation_preserves_original_and_binds_every_body() {
    let (mut plan, mut source) = fixture();
    let body = b"404 original fixture".to_vec();
    let response = binding("discovery/missing.json", &body);
    let review=serde_json::to_vec(&json!({"schema":"ddi-native-optional-color-review/1","sourceSwfSha256":plan.movie.sha256,"missingResources":[{"path":response.path,"sha256":response.sha256,"url":"https://example.test/iro.txt","status":404,"retrievedAt":"2026-10-14"}]})).unwrap();
    let evidence = ddi_danoni_export::extraction::OptionalColorEvidence {
        review: binding("review.json", &review),
        missing_responses: vec![response],
    };
    source.0.insert("review.json".into(), review.clone());
    source
        .0
        .insert("discovery/missing.json".into(), body.clone());
    plan.charts[0].optional_color = Some(evidence);
    let p = prepare_plan(&plan, &source, &mut || Ok(())).unwrap();
    let raw = p
        .files
        .iter()
        .find(|f| f.path == "inputs/original/normal-color.json")
        .unwrap();
    assert_eq!(*raw.bytes, review);
    let derived: Value = serde_json::from_slice(
        &p.files
            .iter()
            .find(|f| f.path == "inputs/normal-color.json")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let dest = derived["missingResources"][0]["path"].as_str().unwrap();
    assert_eq!(dest, format!("inputs/blobs/{}", sha256(&body)));
    assert_eq!(derived["missingResources"][0]["retrievedAt"], "2026-10-14");
    plan.charts[0]
        .optional_color
        .as_mut()
        .unwrap()
        .missing_responses
        .clear();
    assert!(prepare_plan(&plan, &source, &mut || Ok(())).is_err());
}
#[test]
fn no_overwrite_and_failures_do_not_publish_readiness() {
    let (plan, source) = fixture();
    let root = Scratch::new();
    let out = root.0.join("out");
    let worker = WorkerSpec {
        executable: root.0.join("missing-worker"),
        sha256: "0".repeat(64),
        build_identity: root.0.join("missing-build"),
        build_identity_sha256: "0".repeat(64),
    };
    let outcome = execute(&plan, &worker, &source, &out, &mut |_| Ok(())).unwrap();
    assert!(!outcome.ready);
    assert!(out.join("failure.json").exists());
    assert!(!out.join("manifest.json").exists());
    assert_eq!(outcome.manifest["stage"], "capture");
    assert!(out.join("planning.json").exists());
    assert!(execute(&plan, &worker, &source, &out, &mut |_| Ok(())).is_err());
    assert!(out.join("failure.json").exists());
    let cancelled = root.0.join("cancelled");
    let error = execute(&plan, &worker, &source, &cancelled, &mut |p| {
        if p.stage == "staging" {
            Err(Error {
                code: "cancelled",
                message: "test".into(),
            })
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error.code, "cancelled");
    assert!(!cancelled.exists());
}
#[cfg(target_os = "macos")]
#[test]
fn real_worker_transport_success_does_not_make_a_blank_movie_a_chart() {
    let Some(bundle) = std::env::var_os("DDI_DANONI_WORKER_BUNDLE") else {
        return;
    };
    let (mut plan, mut source) = fixture();
    let mut config: Value = serde_json::from_slice(&source.0["inputs/config.json"]).unwrap();
    config["events"] = json!([]);
    config["resources"] = json!([]);
    let raw = serde_json::to_vec(&config).unwrap();
    plan.charts[0].config = binding("inputs/config.json", &raw);
    source.0.insert("inputs/config.json".into(), raw);
    let worker =
        ddi_danoni_export::worker_build::load_bundle(std::path::Path::new(&bundle), &mut || Ok(()))
            .unwrap();
    let root = Scratch::new();
    let out = root.0.join("out");
    let result = execute(&plan, &worker, &source, &out, &mut |_| Ok(())).unwrap();
    assert!(!result.ready);
    assert_eq!(result.manifest["stage"], "normalization");
    assert_eq!(result.manifest["captures"].as_object().unwrap().len(), 1);
    assert!(
        result.manifest["captures"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v["ready"] == true)
    );
    assert!(!out.join("manifest.json").exists());
    assert!(out.join("failure.json").exists());
}
#[cfg(target_os = "macos")]
#[test]
fn parent_readiness_and_late_cancellation_follow_complete_normalization() {
    let (Some(bundle), Some(job)) = (
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
        std::env::var_os("DDI_DANONI_NATIVE_PLAN_JOB"),
    ) else {
        return;
    };
    let root = Scratch::new();
    let out = root.0.join("out");
    let outcome = execute_job(
        std::path::Path::new(&bundle),
        std::path::Path::new(&job),
        &out,
        &mut |p| {
            if p.stage == "ready" {
                Err(Error {
                    code: "cancelled",
                    message: "late test cancellation".into(),
                })
            } else {
                Ok(())
            }
        },
    )
    .unwrap();
    assert!(!outcome.ready);
    assert_eq!(outcome.failure.unwrap().code, "cancelled");
    assert!(out.join("normalized/manifest.json").exists());
    assert!(out.join("failure.json").exists());
    assert!(!out.join("manifest.json").exists());
    assert!(outcome.manifest["normalized"].is_null());
    let ready_out = root.0.join("ready");
    let ready = execute_job(
        std::path::Path::new(&bundle),
        std::path::Path::new(&job),
        &ready_out,
        &mut |_| Ok(()),
    )
    .unwrap();
    assert!(ready.ready);
    assert_eq!(ready.manifest["stage"], "ready");
    assert!(ready_out.join("manifest.json").exists());
    assert!(!ready_out.join("failure.json").exists());
    assert!(!ready_out.join(".manifest.json.tmp").exists());
    let persisted: Value =
        serde_json::from_slice(&fs::read(ready_out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(persisted, ready.manifest);
    assert!(
        execute_job(
            std::path::Path::new(&bundle),
            std::path::Path::new(&job),
            &ready_out,
            &mut |_| Ok(())
        )
        .is_err()
    );
    assert!(ready_out.join("manifest.json").exists());
}
