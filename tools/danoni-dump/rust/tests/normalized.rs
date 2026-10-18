#![cfg(feature = "native")]
//! Original synthetic readiness/inventory checks; no acquired work assets.
use ddi_danoni_export::{
    Error,
    assets::{Artifact, sha256},
    extraction::NormalizedWork,
    native,
};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn temp() -> Temp {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ddi-normalized-{}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&p).unwrap();
    Temp(p.canonicalize().unwrap())
}
fn fixture() -> NormalizedWork {
    let manifest = json!({"schema":"ddi-normalized-work/experimental-1","charts":[]});
    let mut files = vec![
        Artifact {
            path: "source-manifest.json".into(),
            bytes: Arc::new(b"original synthetic source".to_vec()),
        },
        Artifact {
            path: "manifest.json".into(),
            bytes: Arc::new(serde_json::to_vec_pretty(&manifest).unwrap()),
        },
    ];
    let receipt = json!({"schema":"ddi-danoni-native-normalization/1","normalizedManifestSha256":sha256(&files[1].bytes),"artifacts":files.iter().map(|f|json!({"path":f.path,"sha256":sha256(&f.bytes),"bytes":f.bytes.len()})).collect::<Vec<_>>()});
    files.push(Artifact {
        path: "normalization.json".into(),
        bytes: Arc::new(serde_json::to_vec_pretty(&receipt).unwrap()),
    });
    NormalizedWork {
        manifest,
        receipt,
        files,
    }
}
#[test]
fn normalized_writer_is_fresh_and_canceled_partial_outputs_are_removed() {
    let t = temp();
    let work = fixture();
    let output = t.0.join("ready");
    native::write_normalized(&work, &output, &mut || Ok(())).unwrap();
    assert_eq!(
        fs::read(output.join("manifest.json")).unwrap(),
        *work.files[1].bytes
    );
    assert!(!output.join(".manifest.tmp").exists());
    assert!(native::write_normalized(&work, &output, &mut || Ok(())).is_err());
    assert_eq!(
        fs::read(output.join("source-manifest.json")).unwrap(),
        *work.files[0].bytes
    );
    let canceled = t.0.join("canceled");
    let e = native::write_normalized(&work, &canceled, &mut || {
        if canceled.join("source-manifest.json").exists() {
            Err(Error {
                code: "cancelled",
                message: "test".into(),
            })
        } else {
            Ok(())
        }
    })
    .err()
    .unwrap();
    assert_eq!(e.code, "cancelled");
    assert!(!canceled.exists());
    let mut changed = fixture();
    changed.files[0].bytes = Arc::new(b"changed".to_vec());
    let invalid = t.0.join("changed");
    assert_eq!(
        native::write_normalized(&changed, &invalid, &mut || Ok(()))
            .err()
            .unwrap()
            .code,
        "changed-artifact"
    );
    assert!(!invalid.exists());
}
