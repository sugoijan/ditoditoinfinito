#![cfg(feature = "native")]
use ddi_danoni_export::{
    Error, Result, assets::*, dependencies::*, native::DirectorySource, profiles,
    supervisor::WorkerSpec, worker_build,
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ddi-profile-dependencies-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Counted {
    inner: DirectorySource,
    reads: RefCell<BTreeMap<(String, String), usize>>,
}
impl AssetSource for Counted {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        *self
            .reads
            .borrow_mut()
            .entry((ns.into(), path.into()))
            .or_default() += 1;
        self.inner.read(ns, path, max)
    }
}
fn local() -> Option<(ProfileDependencyJob, WorkerSpec, Counted)> {
    let (Some(job), Some(bundle)) = (
        std::env::var_os("DDI_DANONI_PROFILE_DEPENDENCY_JOB"),
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
    ) else {
        eprintln!("SKIP: set DDI_DANONI_PROFILE_DEPENDENCY_JOB and DDI_DANONI_WORKER_BUNDLE");
        return None;
    };
    let path = PathBuf::from(job).canonicalize().unwrap();
    let mut job: ProfileDependencyJob = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(job.schema, "ddi-danoni-native-profile-dependency-job/1");
    for root in job.roots.values_mut() {
        if root.is_relative() {
            *root = path.parent().unwrap().join(&*root);
        }
    }
    let worker = worker_build::load_bundle(Path::new(&bundle), &mut || Ok(())).unwrap();
    let source = Counted {
        inner: DirectorySource::new(job.roots.clone()).unwrap(),
        reads: Default::default(),
    };
    Some((job, worker, source))
}
#[test]
fn required_get_recovers_before_menu_without_refetching_reviewed_404() {
    let Some((job, worker, source)) = local() else {
        return;
    };
    let scratch = Scratch::new();
    let mut calls = vec![];
    let mut counts = vec![];
    let out = profiles::execute_with_dependencies(
        &job.request,
        &worker,
        &source,
        &scratch.0.join("complete"),
        &job.limits,
        &mut |r, checkpoint| {
            checkpoint()?;
            assert!(r.witnesses.iter().all(|w| w.chart_ids.is_empty()));
            assert!(
                r.witnesses
                    .iter()
                    .all(|w| w.snapshot.namespace == "dependency")
            );
            assert!(!r.url.ends_with("iro.txt"));
            calls.push(r.url.clone());
            Ok(job.responses[&r.url].clone())
        },
        &mut |p| {
            counts.push(p.captures);
            Ok(())
        },
    )
    .unwrap();
    assert!(out.ready, "{:?}", out.failure);
    assert_eq!(calls.len(), 1);
    assert!(counts.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(
        out.manifest["schema"],
        "ddi-danoni-native-profile-dependency-run/1"
    );
    assert_eq!(out.manifest["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(out.manifest["counts"]["newUrls"], 1);
    assert!(
        source.reads.borrow().values().all(|n| *n == 1),
        "Each original/response binding read once"
    );
    let completed = &out.manifest["completed"]["normalized"];
    let manifest = out.output.join(completed["manifest"].as_str().unwrap());
    assert_eq!(
        sha256(&fs::read(&manifest).unwrap()),
        completed["manifestSha256"]
    );
    let normalized: Value = serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
    assert_eq!(normalized["charts"].as_array().unwrap().len(), 1);
    let seed: profiles::ProfileJob =
        serde_json::from_slice(&fs::read(out.output.join("seed/profile-job.json")).unwrap())
            .unwrap();
    assert_eq!(seed.request.movie.sha256, job.request.movie.sha256);
    let first: Value =
        serde_json::from_slice(&fs::read(out.output.join("attempts/000/failure.json")).unwrap())
            .unwrap();
    assert_eq!(
        first["failure"]["code"],
        "native-profile-needs-dependencies"
    );
    let second: Value =
        serde_json::from_slice(&fs::read(out.output.join("attempts/001/manifest.json")).unwrap())
            .unwrap();
    let captures = second["captures"].as_object().unwrap().len()
        + first["captures"].as_object().unwrap().len();
    assert_eq!(out.manifest["counts"]["captures"], captures);
    let original_context: Value = serde_json::from_slice(
        &source
            .inner
            .read(
                &job.request.context.namespace,
                &job.request.context.path,
                65536,
            )
            .unwrap(),
    )
    .unwrap();
    for p in fs::read_dir(out.output.join("attempts/001/inputs")).unwrap() {
        let p = p.unwrap().path();
        if !p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("chart-")
        {
            continue;
        }
        let config: Value = serde_json::from_slice(&fs::read(p).unwrap()).unwrap();
        for field in ["sourceUrl", "pageUrl", "flashVars"] {
            assert_eq!(config[field], original_context[field]);
        }
        assert_eq!(config["resources"].as_array().unwrap().len(), 1);
    }
    let replay: ddi_danoni_export::orchestration::PlanJob =
        serde_json::from_slice(&fs::read(out.output.join("attempts/001/replay-job.json")).unwrap())
            .unwrap();
    let staged = DirectorySource::new(BTreeMap::from([(
        "profile".into(),
        out.output.join("attempts/001"),
    )]))
    .unwrap();
    ddi_danoni_export::orchestration::prepare_plan(&replay.plan, &staged, &mut || Ok(())).unwrap();
    assert!(
        profiles::execute_with_dependencies(
            &job.request,
            &worker,
            &source,
            &out.output,
            &job.limits,
            &mut |_, _| unreachable!(),
            &mut |_| Ok(())
        )
        .is_err()
    );
}
#[test]
fn successful_child_cannot_publish_after_outer_cancellation() {
    let Some((job, worker, source)) = local() else {
        return;
    };
    let scratch = Scratch::new();
    let out = profiles::execute_with_dependencies(
        &job.request,
        &worker,
        &source,
        &scratch.0.join("cancel"),
        &job.limits,
        &mut |r, _| Ok(job.responses[&r.url].clone()),
        &mut |p| {
            if p.stage == "ready" {
                Err(Error {
                    code: "cancelled",
                    message: "outer cancellation".into(),
                })
            } else {
                Ok(())
            }
        },
    )
    .unwrap();
    assert!(!out.ready);
    assert_eq!(out.failure.unwrap().code, "cancelled");
    assert!(out.manifest["completed"].is_null());
    assert!(out.output.join("attempts/001/manifest.json").is_file());
    assert!(!out.output.join("manifest.json").exists());
    assert!(out.output.join("failure.json").is_file());
}
#[test]
fn profile_attempt_capture_body_and_output_budgets_span_retries() {
    let Some((job, worker, source)) = local() else {
        return;
    };
    let scratch = Scratch::new();
    for (name, limits, expected_calls, expected_captures) in [
        (
            "attempts",
            DependencyLimits {
                attempts: 1,
                ..job.limits.clone()
            },
            0,
            1,
        ),
        (
            "captures",
            DependencyLimits {
                captures: 2,
                ..job.limits.clone()
            },
            1,
            2,
        ),
        (
            "bytes",
            DependencyLimits {
                new_bytes: 1,
                ..job.limits.clone()
            },
            1,
            1,
        ),
        (
            "output",
            DependencyLimits {
                output_bytes: 64 * 1024 * 1024,
                ..job.limits.clone()
            },
            0,
            1,
        ),
    ] {
        let mut calls = 0;
        let out = profiles::execute_with_dependencies(
            &job.request,
            &worker,
            &source,
            &scratch.0.join(name),
            &limits,
            &mut |r, _| {
                calls += 1;
                if name == "bytes" {
                    assert_eq!(r.maximum_bytes, 1);
                }
                Ok(job.responses[&r.url].clone())
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
        assert_eq!(
            out.manifest["counts"]["captures"], expected_captures,
            "{name}"
        );
        assert!(!out.output.join("manifest.json").exists());
    }
}
#[test]
fn unavailable_host_response_keeps_probe_and_never_creates_dos() {
    let Some((job, worker, source)) = local() else {
        return;
    };
    let scratch = Scratch::new();
    let out = profiles::execute_with_dependencies(
        &job.request,
        &worker,
        &source,
        &scratch.0.join("unavailable"),
        &job.limits,
        &mut |_, _| {
            Err(Error {
                code: "native-dependency-unavailable",
                message: "host cache miss".into(),
            })
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    assert!(!out.ready);
    assert_eq!(out.failure.unwrap().code, "native-dependency-unavailable");
    assert_eq!(out.manifest["counts"]["newUrls"], 0);
    assert!(
        out.output
            .join("attempts/000/probes/menu/capture/snapshot.json")
            .is_file()
    );
    assert!(out.output.join("requests/000.json").is_file());
    assert!(!out.output.join("manifest.json").exists());
}

#[test]
fn changed_probe_receipt_cannot_authorize_a_host_request() {
    let Some((job, worker, source)) = local() else {
        return;
    };
    let scratch = Scratch::new();
    let output = scratch.0.join("changed");
    let mut changed = false;
    let mut calls = 0;
    let out = profiles::execute_with_dependencies(
        &job.request,
        &worker,
        &source,
        &output,
        &job.limits,
        &mut |_, _| {
            calls += 1;
            Err(Error {
                code: "unexpected-resolver",
                message: "untrusted probe".into(),
            })
        },
        &mut |_| {
            let receipt = output.join("attempts/000/probes/menu/manifest.json");
            if !changed && receipt.is_file() {
                let mut v: Value = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
                v["build"] = serde_json::json!({});
                fs::write(receipt, serde_json::to_vec(&v).unwrap()).unwrap();
                changed = true;
            }
            Ok(())
        },
    )
    .unwrap();
    assert!(changed);
    assert_eq!(calls, 0);
    assert!(!out.ready);
    assert_eq!(out.failure.unwrap().code, "invalid-native-dependency");
    assert!(!out.output.join("manifest.json").exists());
}
