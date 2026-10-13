#![cfg(feature = "native")]
//! Source preparation tests use an optional local pinned Git cache. Synthetic
//! Mach-O headers only exercise bundle integrity; they are never executed.
#[cfg(target_os = "macos")]
use ddi_danoni_export::assets::sha256;
use ddi_danoni_export::{Error, worker_build::*};
#[cfg(target_os = "macos")]
use serde_json::{Value, json};
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ddi-worker-build-{}-{}",
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
#[cfg(target_os = "macos")]
fn source() -> Option<PathBuf> {
    static SOURCE: OnceLock<Option<PathBuf>> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            let upstream = std::env::var_os("DDI_DANONI_RUFFLE_SOURCE")?;
            let root = Scratch::new();
            let out = root.0.join("sources");
            prepare_sources(Path::new(&upstream), &out, &mut || Ok(())).unwrap();
            // A process-wide fixture shared read-only by these tests.
            std::mem::forget(root);
            Some(out)
        })
        .clone()
}
#[cfg(target_os = "macos")]
fn copy_sources(root: &Path, dest: &Path) {
    fs::create_dir(dest).unwrap();
    let b = fs::read(root.join("manifest.json")).unwrap();
    let m: Value = serde_json::from_slice(&b).unwrap();
    for row in m["artifacts"].as_array().unwrap() {
        let p = row["path"].as_str().unwrap();
        let to = dest.join(p);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(root.join(p), to).unwrap();
    }
    fs::write(dest.join("manifest.json"), b).unwrap();
}
#[cfg(target_os = "macos")]
fn synthetic_binary(root: &Path) -> (PathBuf, String) {
    let mut b = vec![0; 32];
    b[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
    let cpu = if cfg!(target_arch = "aarch64") {
        0x0100000cu32
    } else {
        0x01000007u32
    };
    b[4..8].copy_from_slice(&cpu.to_le_bytes());
    b[12..16].copy_from_slice(&2u32.to_le_bytes());
    let p = root.join("header-fixture");
    fs::write(&p, &b).unwrap();
    (p, sha256(&b))
}
#[test]
fn nonrepository_rejected_without_output() {
    let root = Scratch::new();
    let out = root.0.join("out");
    assert!(prepare_sources(&root.0, &out, &mut || Ok(())).is_err());
    assert!(!out.exists());
}
#[test]
fn unready_bundle_rejected() {
    let root = Scratch::new();
    assert!(load_bundle(&root.0, &mut || Ok(())).is_err());
}
#[test]
fn pinned_objects_ignore_worktree_edits_and_symlinks() {
    let Some(upstream) = std::env::var_os("DDI_DANONI_RUFFLE_SOURCE") else {
        return;
    };
    let root = Scratch::new();
    let cache = root.0.join("cache");
    let init = Command::new("git")
        .arg("init")
        .arg(&cache)
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    // A local object database alias avoids asking a partial clone to copy
    // unrelated uncached objects. No network is needed by this test.
    fs::write(
        cache.join(".git/objects/info/alternates"),
        format!("{}\n", Path::new(&upstream).join(".git/objects").display()),
    )
    .unwrap();
    fs::create_dir_all(cache.join("core/src")).unwrap();
    fs::write(cache.join("Cargo.toml"), "malicious worktree edit").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/nonexistent/secret", cache.join("core/src/lib.rs")).unwrap();
    let out = root.0.join("out");
    let m = prepare_sources(&cache, &out, &mut || Ok(())).unwrap();
    assert_eq!(m["schema"], "ddi-danoni-worker-sources/1");
    assert_ne!(
        fs::read(out.join("upstream/Cargo.toml")).unwrap(),
        b"malicious worktree edit"
    );
    assert_eq!(
        fs::read(cache.join("Cargo.toml")).unwrap(),
        b"malicious worktree edit"
    );
    let lib = fs::read(out.join("upstream/core/src/lib.rs")).unwrap();
    assert!(lib.ends_with(b"\npub mod ddi_snapshot;\n"));
    assert!(prepare_sources(&cache, &out, &mut || Ok(())).is_err());
    assert!(out.join("manifest.json").exists());
    let cancel = root.0.join("cancel");
    let mut count = 0;
    assert!(
        prepare_sources(&cache, &cancel, &mut || {
            count += 1;
            if count > 10 {
                Err(Error {
                    code: "cancelled",
                    message: "test".into(),
                })
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert!(!cancel.exists());
}
#[cfg(target_os = "macos")]
#[test]
fn package_load_and_explicit_binary_hash() {
    let Some(source) = source() else {
        return;
    };
    let root = Scratch::new();
    let (binary, hash) = synthetic_binary(&root.0);
    let bundle = root.0.join("bundle");
    let m = package_worker(&source, &binary, &hash, &bundle, &mut || Ok(())).unwrap();
    assert_eq!(m["schema"], "ddi-danoni-worker-bundle/1");
    let spec = load_bundle(&bundle, &mut || Ok(())).unwrap();
    assert_eq!(spec.sha256, hash);
    assert_eq!(spec.executable, bundle.join("ddi-ruffle-probe"));
    assert_eq!(
        fs::read(bundle.join("licenses/Ruffle-LICENSE.md")).unwrap(),
        fs::read(source.join("upstream/LICENSE.md")).unwrap()
    );
    assert!(package_worker(&source, &binary, &hash, &bundle, &mut || Ok(())).is_err());
    assert!(bundle.join("manifest.json").exists());
    // A caller changing a file after its inventory read cannot substitute
    // new bytes while the packager copies the earlier verified snapshot.
    let raced = root.0.join("raced-sources");
    copy_sources(&source, &raced);
    let source_manifest: Value =
        serde_json::from_slice(&fs::read(raced.join("manifest.json")).unwrap()).unwrap();
    let threshold = source_manifest["artifacts"].as_array().unwrap().len() + 1;
    let old_lock = fs::read(raced.join("worker/Cargo.lock")).unwrap();
    let mut checkpoints = 0;
    let mut changed = false;
    let raced_bundle = root.0.join("raced-bundle");
    package_worker(&raced, &binary, &hash, &raced_bundle, &mut || {
        checkpoints += 1;
        if checkpoints > threshold && !changed {
            fs::write(raced.join("worker/Cargo.lock"), b"changed after inventory").unwrap();
            changed = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(changed);
    assert_eq!(
        fs::read(raced_bundle.join("provenance/Cargo.lock")).unwrap(),
        old_lock
    );
    load_bundle(&raced_bundle, &mut || Ok(())).unwrap();
    let cancelled = root.0.join("cancelled-stage");
    assert!(
        package_worker(&source, &binary, &hash, &cancelled, &mut || {
            if cancelled.join("worker.sb").exists() {
                Err(Error {
                    code: "cancelled",
                    message: "test".into(),
                })
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert!(!cancelled.exists());
    let invalid = root.0.join("wrong-hash");
    assert!(package_worker(&source, &binary, &"0".repeat(64), &invalid, &mut || Ok(())).is_err());
    assert!(!invalid.exists());
    let invalid = root.0.join("wrong-platform");
    fs::write(&binary, b"FWS fake binary").unwrap();
    let hash = sha256(b"FWS fake binary");
    assert!(package_worker(&source, &binary, &hash, &invalid, &mut || Ok(())).is_err());
    assert!(!invalid.exists());
}
#[cfg(target_os = "macos")]
#[test]
fn changed_or_uninventoried_sources_are_not_packaged() {
    let Some(source) = source() else {
        return;
    };
    let root = Scratch::new();
    let (binary, hash) = synthetic_binary(&root.0);
    for kind in [
        "changed",
        "uninventoried",
        "forged-worker",
        "symlink",
        "cancelled",
    ] {
        let sources = root.0.join(kind);
        copy_sources(&source, &sources);
        let out = root.0.join(format!("{kind}-bundle"));
        match kind {
            "changed" => fs::write(sources.join("upstream/core/src/lib.rs"), "changed").unwrap(),
            "uninventoried" => fs::write(sources.join("worker/build.rs"), "fn main() {}").unwrap(),
            "forged-worker" => {
                let path = "worker/src/main.rs";
                let data = b"fn main() {}";
                fs::write(sources.join(path), data).unwrap();
                let mut m: Value =
                    serde_json::from_slice(&fs::read(sources.join("manifest.json")).unwrap())
                        .unwrap();
                for r in m["artifacts"].as_array_mut().unwrap() {
                    if r["path"] == path {
                        r["sha256"] = json!(sha256(data));
                        r["bytes"] = json!(data.len());
                    }
                }
                fs::write(
                    sources.join("manifest.json"),
                    serde_json::to_vec(&m).unwrap(),
                )
                .unwrap();
            }
            "symlink" => {
                fs::remove_file(sources.join("upstream/LICENSE.md")).unwrap();
                std::os::unix::fs::symlink(
                    source.join("upstream/LICENSE.md"),
                    sources.join("upstream/LICENSE.md"),
                )
                .unwrap();
            }
            _ => (),
        }
        let mut count = 0;
        let result = package_worker(&sources, &binary, &hash, &out, &mut || {
            count += 1;
            if kind == "cancelled" && count > 10 {
                Err(Error {
                    code: "cancelled",
                    message: "test".into(),
                })
            } else {
                Ok(())
            }
        });
        assert!(result.is_err(), "{kind}");
        assert!(!out.exists(), "{kind}");
    }
}
#[cfg(target_os = "macos")]
#[test]
fn bundle_integrity_requires_full_inventory_and_policy() {
    let Some(source) = source() else {
        return;
    };
    let root = Scratch::new();
    let (binary, hash) = synthetic_binary(&root.0);
    for kind in [
        "changed",
        "extra",
        "missing-row",
        "forged-policy",
        "symlink",
    ] {
        let bundle = root.0.join(kind);
        package_worker(&source, &binary, &hash, &bundle, &mut || Ok(())).unwrap();
        match kind {
            "changed" => fs::write(bundle.join("build-identity.json"), "{}").unwrap(),
            "extra" => fs::write(bundle.join("extra"), "unexpected").unwrap(),
            "missing-row" => {
                let mut m: Value =
                    serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap())
                        .unwrap();
                m["artifacts"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|r| r["path"] != "ddi-ruffle-probe");
                fs::write(
                    bundle.join("manifest.json"),
                    serde_json::to_vec(&m).unwrap(),
                )
                .unwrap();
            }
            "forged-policy" => {
                let b = b"(allow default)";
                fs::write(bundle.join("worker.sb"), b).unwrap();
                let mut m: Value =
                    serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap())
                        .unwrap();
                for r in m["artifacts"].as_array_mut().unwrap() {
                    if r["path"] == "worker.sb" {
                        r["sha256"] = json!(sha256(b));
                        r["bytes"] = json!(b.len());
                    }
                }
                fs::write(
                    bundle.join("manifest.json"),
                    serde_json::to_vec(&m).unwrap(),
                )
                .unwrap();
            }
            "symlink" => {
                fs::remove_file(bundle.join("provenance/Cargo.lock")).unwrap();
                std::os::unix::fs::symlink(
                    source.join("worker/Cargo.lock"),
                    bundle.join("provenance/Cargo.lock"),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(load_bundle(&bundle, &mut || Ok(())).is_err(), "{kind}");
    }
}
#[cfg(target_os = "macos")]
#[test]
fn installed_bundle_runs_original_transport_under_supervision() {
    let Some(bundle) = std::env::var_os("DDI_DANONI_WORKER_BUNDLE") else {
        return;
    };
    use ddi_danoni_export::{
        assets::Binding,
        native::DirectorySource,
        supervisor::{CaptureRequest, Limits, capture},
    };
    use std::collections::BTreeMap;
    let worker = load_bundle(Path::new(&bundle), &mut || Ok(())).unwrap();
    let root = Scratch::new();
    let inputs = root.0.join("inputs");
    fs::create_dir(&inputs).unwrap();
    let mut movie = b"FWS\x06".to_vec();
    movie.extend(18u32.to_le_bytes());
    movie.extend([0x08, 0, 0, 30, 1, 0, 0x40, 0, 0, 0]);
    fs::write(inputs.join("fixture.swf"), &movie).unwrap();
    let config=serde_json::to_vec(&json!({"movie":"fixture.swf","sourceUrl":"https://example.test/bundle.swf","captureFrames":[1,2],"resources":[]})).unwrap();
    fs::write(inputs.join("config.json"), &config).unwrap();
    let source = DirectorySource::new(BTreeMap::from([("source".into(), inputs)])).unwrap();
    let request = CaptureRequest {
        config: Binding {
            namespace: "source".into(),
            path: "config.json".into(),
            sha256: sha256(&config),
        },
        movie: Binding {
            namespace: "source".into(),
            path: "fixture.swf".into(),
            sha256: sha256(&movie),
        },
        provenance: None,
        limits: Limits::default(),
    };
    let out = root.0.join("capture");
    let result = capture(&request, &worker, &source, &out, &mut || Ok(())).unwrap();
    assert!(result.ready, "{:?}", result.failure);
    assert!(out.join("manifest.json").exists());
    assert!(!out.join("failure.json").exists());
    let snapshot: Value =
        serde_json::from_slice(&fs::read(out.join("capture/snapshot.json")).unwrap()).unwrap();
    assert_eq!(snapshot["captures"].as_array().unwrap().len(), 2);
    assert_eq!(snapshot["swfHeaderFps"].as_f64(), Some(30.));
}
