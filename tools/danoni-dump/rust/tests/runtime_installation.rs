#![cfg(feature = "native")]
//! Optional installed-byte integrity gate; no corpus execution or live fetches.
use ddi_danoni_export::{assets::sha256, runtime::Installation};
use serde_json::{Value, json};
use std::{fs, path::Path};
fn copy(source: &Path, dest: &Path) {
    fs::create_dir(dest).unwrap();
    for e in fs::read_dir(source).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        let d = dest.join(e.file_name());
        assert!(!e.file_type().unwrap().is_symlink());
        if p.is_dir() {
            copy(&p, &d);
        } else {
            fs::copy(p, d).unwrap();
        }
    }
}
#[test]
fn installed_inventory_version_fonts_policy_and_relocation_fail_closed() {
    let Some(prefix) = std::env::var_os("DDI_DANONI_INSTALLED_RUNTIME") else {
        eprintln!("SKIP: set DDI_DANONI_INSTALLED_RUNTIME");
        return;
    };
    let install = Installation::load(Path::new(&prefix)).unwrap();
    let root = std::env::temp_dir().join(format!("ddi-runtime-integrity-{}", std::process::id()));
    copy(&install.root, &root);
    Installation::load(&root).unwrap(); // Prefix relocation.
    let manifest = fs::read(root.join("manifest.json")).unwrap();
    let license = root.join("licenses/Noto-OFL.txt");
    let original = fs::read(&license).unwrap();
    fs::write(&license, b"changed").unwrap();
    assert!(Installation::load(&root).is_err());
    fs::write(&license, &original).unwrap();
    let mut m: Value = serde_json::from_slice(&manifest).unwrap();
    m["identity"]["sdkVersion"] = json!("wrong-version");
    fs::write(root.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(Installation::load(&root).is_err());
    fs::write(root.join("manifest.json"), &manifest).unwrap();
    fs::write(root.join("unexpected"), b"extra").unwrap();
    assert!(Installation::load(&root).is_err());
    fs::remove_file(root.join("unexpected")).unwrap();
    let font = root.join("fonts/NotoSansCJKjp-Regular.otf");
    let moved = root.join("removed-font");
    fs::rename(&font, &moved).unwrap();
    assert!(Installation::load(&root).is_err());
    fs::rename(&moved, &font).unwrap();
    #[cfg(unix)]
    {
        fs::rename(&license, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &license).unwrap();
        assert!(Installation::load(&root).is_err());
        fs::remove_file(&license).unwrap();
        fs::rename(&moved, &license).unwrap();
    }
    // Even a rewritten outer inventory cannot authorize a different policy.
    let policy = if cfg!(target_os = "linux") {
        "worker/worker-linux.rs"
    } else {
        "worker/worker.sb"
    };
    let original = fs::read(root.join(policy)).unwrap();
    fs::write(root.join(policy), b"relaxed").unwrap();
    let mut m: Value = serde_json::from_slice(&manifest).unwrap();
    let row = m["artifacts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["path"] == policy)
        .unwrap();
    row["sha256"] = json!(sha256(b"relaxed"));
    row["bytes"] = json!(7);
    fs::write(root.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(Installation::load(&root).is_err());
    fs::write(root.join(policy), original).unwrap();
    fs::write(root.join("manifest.json"), manifest).unwrap();
    Installation::load(&root).unwrap();
    fs::remove_dir_all(root).unwrap();
}
