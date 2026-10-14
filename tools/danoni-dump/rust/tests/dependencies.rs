#![cfg(feature = "native")]
use ddi_danoni_export::{
    Error, Result,
    assets::*,
    dependencies::*,
    orchestration::NativePlan,
    supervisor::{self, WorkerSpec},
    worker_build,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Memory(BTreeMap<String, Vec<u8>>, RefCell<BTreeMap<String, usize>>);
impl AssetSource for Memory {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        assert_eq!(ns, "test");
        *self.1.borrow_mut().entry(path.into()).or_default() += 1;
        relative_path(path)?;
        let b = self.0.get(path).ok_or_else(|| Error {
            code: "missing-test-input",
            message: path.into(),
        })?;
        assert!(b.len() <= max);
        Ok(b.clone())
    }
}
fn binding(path: &str, b: &[u8]) -> Binding {
    Binding {
        namespace: "test".into(),
        path: path.into(),
        sha256: sha256(b),
    }
}
fn snapshot() -> Value {
    json!({"schema":"ddi-ruffle-probe/experimental-1","build":supervisor::expected_build_identity(),"ruffleRevision":"89a7049965f3090e706265670df309a5128233ac","avm1Halted":false,
        "source":{"config":{"movie":"movie.swf","sourceUrl":"https://example.test/work/source.swf?name=num301","captureFrames":[1,2],"resources":[]}},
        "audit":{"truncated":false,"droppedEntries":0,"truncatedDetails":0,"runtimeErrors":0,"entries":[{"iteration":1,"kind":"fetch-denied","detail":"dos.txt?name=num301","truncated":false,
        "request":{"schema":"ddi-native-request/1","method":"GET","hasBody":false,"hasHeaders":false,"originalUrl":"dos.txt?name=num301","resolvedUrl":"https://example.test/work/dos.txt?name=num301","reason":"cache-miss"}}]}})
}
#[test]
fn only_structured_bound_get_misses_are_dependencies() {
    let s = snapshot();
    let requests = inspect_requests(&s).unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url,
        "https://example.test/work/dos.txt?name=num301"
    );
    for key in [
        "build",
        "url-only",
        "method",
        "body",
        "headers",
        "reason",
        "resolution",
        "userinfo",
        "fragment",
        "scheme",
        "original",
        "iteration",
        "truncated",
        "audit",
        "runtime-count",
        "unknown",
        "already-cached",
    ] {
        let mut s = s.clone();
        match key {
            "build" => s["build"] = json!({}),
            "url-only" => {
                s["audit"]["entries"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("request");
            }
            "method" => s["audit"]["entries"][0]["request"]["method"] = json!("POST"),
            "body" => s["audit"]["entries"][0]["request"]["hasBody"] = json!(true),
            "headers" => s["audit"]["entries"][0]["request"]["hasHeaders"] = json!(true),
            "reason" => s["audit"]["entries"][0]["request"]["reason"] = json!("method-denied"),
            "resolution" => {
                s["audit"]["entries"][0]["request"]["resolvedUrl"] =
                    json!("https://example.test/other")
            }
            "userinfo" => {
                s["audit"]["entries"][0]["request"]["resolvedUrl"] =
                    json!("https://u:p@example.test/a")
            }
            "fragment" => {
                s["audit"]["entries"][0]["request"]["resolvedUrl"] =
                    json!("https://example.test/a#fragment")
            }
            "scheme" => {
                s["audit"]["entries"][0]["request"]["resolvedUrl"] = json!("file:///tmp/secret")
            }
            "original" => s["audit"]["entries"][0]["request"]["originalUrl"] = json!("other.txt"),
            "iteration" => s["audit"]["entries"][0]["iteration"] = json!(3),
            "truncated" => s["audit"]["entries"][0]["truncated"] = json!(true),
            "audit" => s["audit"]["truncated"] = json!(true),
            "runtime-count" => s["audit"]["runtimeErrors"] = json!(1),
            "unknown" => s["audit"]["entries"][0]["request"]["arbitrary"] = json!(true),
            "already-cached" => {
                s["source"]["config"]["resources"] = json!([{"url":"https://example.test/work/dos.txt?name=num301","path":"dos.txt","sha256":"0".repeat(64)}])
            }
            _ => unreachable!(),
        }
        assert!(inspect_requests(&s).is_err(), "{key}");
    }
}
#[test]
fn traces_errors_navigation_and_sockets_never_authorize_a_fetch() {
    for kind in ["trace", "navigation-denied", "socket-denied", "invented"] {
        let mut s = snapshot();
        s["audit"]["entries"][0]["kind"] = json!(kind);
        assert!(inspect_requests(&s).is_err(), "{kind}");
    }
    let mut s = snapshot();
    s["audit"]["entries"][0] =
        json!({"kind":"runtime-error","iteration":1,"truncated":false,"detail":"fetch dos.txt"});
    s["audit"]["runtimeErrors"] = json!(1);
    assert!(inspect_requests(&s).unwrap().is_empty());
}
// Original, minimal AVM1 fixture. Stop after issuing cached loadVariables GETs.
// No downloaded program or third-party media is a fixture.
fn action(op: u8, p: &[u8]) -> Vec<u8> {
    let mut b = vec![op];
    if op >= 128 {
        b.extend((p.len() as u16).to_le_bytes());
        b.extend(p);
    }
    b
}
fn push(s: &str) -> Vec<u8> {
    let mut b = vec![0];
    b.extend(s.as_bytes());
    b.push(0);
    action(0x96, &b)
}
fn tag(code: u16, p: &[u8]) -> Vec<u8> {
    let mut b = ((code << 6) | p.len().min(63) as u16)
        .to_le_bytes()
        .to_vec();
    if p.len() >= 63 {
        b.extend((p.len() as u32).to_le_bytes());
    }
    b.extend(p);
    b
}
fn fixture(post: bool) -> (NativePlan, Memory) {
    let mut code = Vec::new();
    for name in ["a.txt?name=num301", "b.txt"] {
        code.extend(push(name));
        code.extend(push("_root"));
        code.extend(action(0x9a, &[if post { 0xc2 } else { 0xc0 }]));
    }
    code.extend([7, 0]);
    let mut body = vec![0x08, 0, 0, 60, 1, 0];
    body.extend(tag(12, &code));
    body.extend(tag(1, &[]));
    body.extend(tag(0, &[]));
    let mut movie = b"FWS\x06".to_vec();
    movie.extend((8 + body.len() as u32).to_le_bytes());
    movie.extend(body);
    let provenance = serde_json::to_vec(&json!({"sourceSwfSha256":sha256(&movie)})).unwrap();
    let config=serde_json::to_vec(&json!({"movie":"movie.swf","sourceUrl":"https://example.test/work/source.swf?name=num301","flashVars":{"name":"num301"},"captureFrames":[1,2],"resources":[],"mixAudio":true})).unwrap();
    let row = json!({"id":"normal","options":{"family":"child-timeline-5","mode":"5","sourceSuffix":null,"pinnedSelection":null},"config":binding("config.json",&config),"projection":{"captureFrames":[1,2],"mixAudio":true},"motion":{"captureFrames":[1,2],"mixAudio":true},"audio":{"captureFrames":[1,2],"mixAudio":true},"motionSelection":{"note":"leftMC2","receptor":"left_step","lane":0,"iterations":[1,2]},"streamStartIndex":0});
    let plan=serde_json::from_value(json!({"movie":binding("movie.swf",&movie),"sourceManifest":binding("source.json",&provenance),"charts":[row]})).unwrap();
    (
        plan,
        Memory(
            BTreeMap::from([
                ("movie.swf".into(), movie),
                ("source.json".into(), provenance),
                ("config.json".into(), config),
            ]),
            RefCell::new(BTreeMap::new()),
        ),
    )
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ddi-dependencies-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p.canonicalize().unwrap())
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn worker() -> Option<WorkerSpec> {
    let bundle = std::env::var_os("DDI_DANONI_WORKER_BUNDLE")?;
    Some(worker_build::load_bundle(&PathBuf::from(bundle), &mut || Ok(())).unwrap())
}
fn responses(source: &mut Memory) -> BTreeMap<String, DependencyResponse> {
    let mut result = BTreeMap::new();
    for (i, url) in [
        "https://example.test/work/a.txt?name=num301",
        "https://example.test/work/b.txt",
    ]
    .iter()
    .enumerate()
    {
        let body = b"original_fixture=value".to_vec();
        let provenance=serde_json::to_vec(&json!({"schema":"ddi-danoni-dependency-response/1","logicalUrl":url,"retrievalUrl":url,"retrievedAt":"2026-10-14T00:00:00Z","status":200,"sha256":sha256(&body),"policyDecision":"original fixture cache; network disabled"})).unwrap();
        let b = format!("body-{i}");
        let p = format!("response-{i}.json");
        result.insert(
            (*url).into(),
            DependencyResponse {
                body: binding(&b, &body),
                provenance: binding(&p, &provenance),
                http_charset: Some("Shift_JIS".into()),
            },
        );
        source.0.insert(b, body);
        source.0.insert(p, provenance);
    }
    result
}
#[test]
fn bad_limits_or_source_fail_before_creating_output() {
    let (plan, source) = fixture(false);
    let root = Temp::new();
    let dummy = WorkerSpec {
        executable: root.0.join("none"),
        sha256: "0".repeat(64),
        build_identity: root.0.join("none"),
        build_identity_sha256: "0".repeat(64),
    };
    for bad in [
        DependencyLimits {
            attempts: 0,
            ..Default::default()
        },
        DependencyLimits {
            captures: 65,
            ..Default::default()
        },
        DependencyLimits {
            wall_ms: 300001,
            ..Default::default()
        },
        DependencyLimits {
            new_bytes: 65 * 1024 * 1024,
            ..Default::default()
        },
    ] {
        assert!(
            execute(
                &plan,
                &dummy,
                &source,
                &root.0.join("out"),
                &bad,
                &mut |_, _| unreachable!(),
                &mut |_| Ok(())
            )
            .is_err()
        );
        assert!(!root.0.join("out").exists());
    }
    let mut bad = plan.clone();
    bad.movie.sha256 = "0".repeat(64);
    assert!(
        execute(
            &bad,
            &dummy,
            &source,
            &root.0.join("out"),
            &Default::default(),
            &mut |_, _| unreachable!(),
            &mut |_| Ok(())
        )
        .is_err()
    );
    assert!(!root.0.join("out").exists());
}
#[test]
fn real_worker_closes_shared_dependencies_but_never_invents_a_chart() {
    let Some(worker) = worker() else { return };
    let (mut plan, mut source) = fixture(false);
    let mut alternate = plan.charts[0].clone();
    alternate.id = "alternate".into();
    plan.charts.push(alternate);
    let responses = responses(&mut source);
    let root = Temp::new();
    let mut calls = Vec::new();
    let out = execute(
        &plan,
        &worker,
        &source,
        &root.0.join("out"),
        &Default::default(),
        &mut |request, checkpoint| {
            checkpoint()?;
            calls.push(request.url.clone());
            assert!(request.witnesses.iter().all(|w| w.chart_ids.len() == 2));
            Ok(responses[&request.url].clone())
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    assert!(!out.ready);
    assert_eq!(out.manifest["counts"]["newUrls"], 2);
    assert_eq!(out.manifest["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(calls.len(), 2);
    assert!(!out.output.join("manifest.json").exists());
    assert!(out.output.join("failure.json").exists());
    let replay: Value =
        serde_json::from_slice(&fs::read(out.output.join("attempts/001/replay-job.json")).unwrap())
            .unwrap();
    let config = &replay["plan"]["charts"][0]["config"]["path"]
        .as_str()
        .unwrap();
    let c: Value =
        serde_json::from_slice(&fs::read(out.output.join("attempts/001").join(config)).unwrap())
            .unwrap();
    assert_eq!(c["flashVars"]["name"], "num301");
    assert_eq!(c["resources"].as_array().unwrap().len(), 2);
    assert_eq!(c["resources"][0]["httpCharset"], "Shift_JIS");
    assert!(
        source.1.borrow().values().all(|reads| *reads == 1),
        "Each original/cache input is read only once per operation"
    );
    assert!(
        execute(
            &plan,
            &worker,
            &source,
            &out.output,
            &Default::default(),
            &mut |_, _| unreachable!(),
            &mut |_| Ok(())
        )
        .is_err()
    );
}
#[test]
fn attempt_capture_url_byte_and_cancel_budgets_keep_failure_receipts() {
    let Some(worker) = worker() else { return };
    let (plan, mut source) = fixture(false);
    let responses = responses(&mut source);
    let root = Temp::new();
    let output_limited = execute(
        &plan,
        &worker,
        &source,
        &root.0.join("output"),
        &DependencyLimits {
            output_bytes: 1024 * 1024,
            ..Default::default()
        },
        &mut |_, _| unreachable!(),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(
        output_limited.failure.unwrap().code,
        "native-dependency-limit"
    );
    assert!(!output_limited.output.join("manifest.json").exists());
    assert!(
        output_limited.manifest["attempts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for (name, limits, expected_calls) in [
        (
            "attempts",
            DependencyLimits {
                attempts: 1,
                ..Default::default()
            },
            0,
        ),
        (
            "captures",
            DependencyLimits {
                captures: 1,
                ..Default::default()
            },
            2,
        ),
        (
            "urls",
            DependencyLimits {
                new_urls: 1,
                ..Default::default()
            },
            1,
        ),
        (
            "bytes",
            DependencyLimits {
                new_bytes: 1,
                ..Default::default()
            },
            1,
        ),
    ] {
        let mut calls = 0;
        let out = execute(
            &plan,
            &worker,
            &source,
            &root.0.join(name),
            &limits,
            &mut |r, _| {
                calls += 1;
                Ok(responses[&r.url].clone())
            },
            &mut |_| Ok(()),
        )
        .unwrap();
        assert!(!out.ready, "{name}");
        assert_eq!(
            out.failure.unwrap().code,
            "native-dependency-limit",
            "{name}"
        );
        assert_eq!(calls, expected_calls, "{name}");
        assert!(!out.output.join("manifest.json").exists());
        assert!(out.output.join("failure.json").exists());
    }
    let out = execute(
        &plan,
        &worker,
        &source,
        &root.0.join("cancel"),
        &Default::default(),
        &mut |_, _| {
            Err(Error {
                code: "cancelled",
                message: "host cancelled".into(),
            })
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(out.failure.unwrap().code, "cancelled");
    assert!(!out.output.join("manifest.json").exists());
}
#[test]
fn time_in_host_callbacks_counts_against_the_operation_deadline() {
    let (plan, source) = fixture(false);
    let root = Temp::new();
    let worker = WorkerSpec {
        executable: root.0.join("none"),
        sha256: "0".repeat(64),
        build_identity: root.0.join("none"),
        build_identity_sha256: "0".repeat(64),
    };
    let failure = execute(
        &plan,
        &worker,
        &source,
        &root.0.join("out"),
        &DependencyLimits {
            wall_ms: 1,
            ..Default::default()
        },
        &mut |_, _| unreachable!(),
        &mut |_| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(failure.code, "native-dependency-limit");
    assert!(!root.0.join("out").exists());
}
#[test]
fn invalid_provenance_non_success_and_post_are_not_acquired() {
    let Some(worker) = worker() else { return };
    let root = Temp::new();
    for kind in ["status", "hash", "logical", "policy", "charset", "post"] {
        let (plan, mut source) = fixture(kind == "post");
        let mut responses = responses(&mut source);
        if kind != "post" {
            for r in responses.values_mut() {
                let mut p: Value = serde_json::from_slice(&source.0[&r.provenance.path]).unwrap();
                match kind {
                    "status" => p["status"] = json!(404),
                    "logical" => p["logicalUrl"] = json!("https://example.test/wrong"),
                    "policy" => p["policyDecision"] = json!(""),
                    "hash" => p["sha256"] = json!("0".repeat(64)),
                    "charset" => r.http_charset = Some("nonsense-charset".into()),
                    _ => unreachable!(),
                }
                let b = serde_json::to_vec(&p).unwrap();
                r.provenance.sha256 = sha256(&b);
                source.0.insert(r.provenance.path.clone(), b);
            }
        }
        let mut calls = 0;
        let out = execute(
            &plan,
            &worker,
            &source,
            &root.0.join(kind),
            &Default::default(),
            &mut |r, _| {
                calls += 1;
                Ok(responses[&r.url].clone())
            },
            &mut |_| Ok(()),
        )
        .unwrap();
        assert!(!out.ready, "{kind}");
        if kind == "post" {
            assert_eq!(calls, 0);
        }
        assert_eq!(out.manifest["counts"]["newUrls"], 0, "{kind}");
        assert!(!out.output.join("manifest.json").exists());
    }
}
#[test]
fn real_higan_closure_and_late_cancel_respect_parent_readiness() {
    let (Some(bundle), Some(job)) = (
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
        std::env::var_os("DDI_DANONI_DEPENDENCY_JOB"),
    ) else {
        return;
    };
    let root = Temp::new();
    let out = execute_job(
        &PathBuf::from(&bundle),
        &PathBuf::from(&job),
        &root.0.join("ready"),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert!(out.ready, "{:?}", out.failure);
    assert_eq!(out.manifest["counts"]["newUrls"], 2);
    assert_eq!(out.manifest["attempts"].as_array().unwrap().len(), 2);
    assert!(out.output.join("manifest.json").exists());
    let cancelled = execute_job(
        &PathBuf::from(bundle),
        &PathBuf::from(job),
        &root.0.join("cancelled"),
        &mut |p| {
            if p.stage == "ready" {
                Err(Error {
                    code: "cancelled",
                    message: "final checkpoint".into(),
                })
            } else {
                Ok(())
            }
        },
    )
    .unwrap();
    assert!(!cancelled.ready);
    assert_eq!(cancelled.failure.unwrap().code, "cancelled");
    assert!(!cancelled.output.join("manifest.json").exists());
    assert!(cancelled.output.join("attempts/001/manifest.json").exists());
}
