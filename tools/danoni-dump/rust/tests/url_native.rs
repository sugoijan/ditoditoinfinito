#![cfg(feature = "native")]
//! Optional local-only whole URL readiness check with the installed real worker.
use ddi_danoni_export::{Error, assets::*, network::NetworkCancel, url_native::*, worker_build};
use serde_json::Value;
use std::{fs, path::PathBuf};
#[test]
fn real_url_child_cannot_publish_parent_after_late_cancellation() {
    let (Some(job), Some(bundle)) = (
        std::env::var_os("DDI_DANONI_URL_JOB"),
        std::env::var_os("DDI_DANONI_WORKER_BUNDLE"),
    ) else {
        return;
    };
    let path = PathBuf::from(job);
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let mut request: UrlRequest = serde_json::from_value(doc["request"].clone()).unwrap();
    assert!(
        request.fetch.offline,
        "Local URL test must never fetch a work"
    );
    request.selectors = Some(vec![0]);
    let worker = worker_build::load_bundle(&PathBuf::from(bundle), &mut || Ok(())).unwrap();
    let cache = PathBuf::from(doc["cache"].as_str().unwrap());
    let cache = if cache.is_absolute() {
        cache
    } else {
        path.parent().unwrap().join(cache)
    };
    let output = std::env::temp_dir().join(format!(
        "ddi-url-ready-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let out = execute(
        &request,
        &cache,
        &worker,
        &output,
        &NetworkCancel::default(),
        &mut |_| panic!("Offline URL test contacted site policy"),
        &mut |p| {
            if p.stage == "ready" {
                assert!(output.join(".manifest.json.tmp").is_file());
                assert!(!output.join("manifest.json").exists());
                Err(Error {
                    code: "host-late-cancel",
                    message: "fixture parent boundary".into(),
                })
            } else {
                Ok(())
            }
        },
    )
    .unwrap();
    assert!(!out.ready);
    assert_eq!(out.failure.unwrap().code, "host-late-cancel");
    assert!(out.manifest["completed"].is_null());
    assert!(!output.join("manifest.json").exists());
    assert!(!output.join(".manifest.json.tmp").exists());
    assert!(output.join("failure.json").is_file());
    assert!(output.join("native/manifest.json").is_file());
    assert_eq!(out.manifest["transfers"]["requests"], 0);
    for item in out.manifest["inputs"].as_array().unwrap() {
        let binding: Binding = serde_json::from_value(item.clone()).unwrap();
        relative_path(&binding.path).unwrap();
        assert_eq!(
            sha256(&fs::read(output.join("acquisition").join(binding.path)).unwrap()),
            binding.sha256
        );
    }
    if let Some(destination) = std::env::var_os("DDI_DANONI_URL_CANCEL_RECEIPT") {
        let receipt = serde_json::json!({"output":output,"manifest":out.manifest});
        fs::write(destination, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    } else {
        fs::remove_dir_all(output).unwrap();
    }
}
