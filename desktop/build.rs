//! Build-time preparation only. No SDK is linked into the desktop binary.
use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rustc-env=DDI_DANONI_BUNDLE_ID=");
    println!("cargo:rustc-env=DDI_DANONI_BUNDLE_MANIFEST_SHA256=");
    for name in [
        "DDI_IMPORTER_TEST_ONLY",
        "DDI_IMPORTER_BUILD_CACHE",
        "DDI_IMPORTER_UPSTREAM",
        "DDI_IMPORTER_WORKER_BUNDLE",
        "DDI_IMPORTER_WORKER_SOURCES",
        "CARGO_NET_OFFLINE",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "MACOSX_DEPLOYMENT_TARGET",
        "SDKROOT",
        "CC",
        "CXX",
        "AR",
        "CFLAGS",
        "CXXFLAGS",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let target = env::var("TARGET").unwrap();
    let cargo_target = target.to_uppercase().replace('-', "_");
    for suffix in ["LINKER", "RUSTFLAGS"] {
        println!("cargo:rerun-if-env-changed=CARGO_TARGET_{cargo_target}_{suffix}");
    }
    for prefix in ["CC", "CXX", "AR", "CFLAGS", "CXXFLAGS"] {
        for suffix in [target.clone(), target.replace('-', "_")] {
            println!("cargo:rerun-if-env-changed={prefix}_{suffix}");
        }
        for scope in ["HOST", "TARGET"] {
            println!("cargo:rerun-if-env-changed={scope}_{prefix}");
        }
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .to_owned();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    // Native artifacts use the exact checkout's assets even with an arbitrary
    // CARGO_TARGET_DIR. Nothing in the shipped executable remembers that path.
    println!("cargo:rerun-if-changed={}", root.join("assets").display());
    copy_assets(&root.join("assets"), &profile.join("assets"));
    for name in ["songs", "noteskins"] {
        let index = root.join("target").join(name).join("index.json");
        println!("cargo:rerun-if-changed={}", index.display());
        if index.is_file() {
            let destination = profile.join("assets").join(name).join("index.json");
            copy_file(&index, &destination);
        }
    }
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if env::var_os("CARGO_FEATURE_DANONI_IMPORTER").is_none()
        || !(os == "macos" || (os == "linux" && arch == "x86_64"))
    {
        return;
    }
    if env::var("DDI_IMPORTER_TEST_ONLY").as_deref() == Ok("1") {
        assert_eq!(
            profile.file_name().and_then(|name| name.to_str()),
            Some("debug"),
            "Test-only importer builds require the ordinary debug profile directory"
        );
        assert_eq!(
            env::var("PROFILE").as_deref(),
            Ok("debug"),
            "Test-only importer builds are forbidden in release/custom profiles"
        );
        println!(
            "cargo:warning=Test-only importer build: no runtime is staged and URL import remains unavailable"
        );
        return;
    }
    assert_eq!(
        env::var("HOST"),
        env::var("TARGET"),
        "Build the importer on its native target host; cross builds must not package a host worker"
    );
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "chart/src",
        "chart/Cargo.toml",
        "engine/src",
        "engine/Cargo.toml",
        "library/src",
        "library/Cargo.toml",
        "platform/src",
        "platform/Cargo.toml",
        "importer-protocol",
        "tools/danoni-importer/src",
        "tools/danoni-importer/Cargo.toml",
        "tools/danoni-importer/build.py",
        "tools/danoni-importer/notices.py",
        "tools/danoni-importer/licenses",
        "LICENSES",
        "REUSE.toml",
        "THIRD_PARTY.toml",
        "CREDITS.md",
        "tools/danoni-dump/rust/src",
        "tools/danoni-dump/rust/fonts",
        "tools/danoni-dump/rust/reviews",
        "tools/danoni-dump/rust/Cargo.toml",
        "tools/danoni-dump/ruffle",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    println!(
        "cargo:warning=Preparing the native DanOni importer bundle (cached; first build also prepares the pinned worker)"
    );
    let result = Command::new("python3")
        .arg(root.join("tools/danoni-importer/build.py"))
        .arg("prepare")
        .arg("--target")
        .arg(env::var("TARGET").unwrap())
        .arg("--stage")
        .arg(profile)
        .current_dir(&root)
        // Nested Cargo builds must never share the parent's jobserver or target lock.
        .env_remove("CARGO_MAKEFLAGS")
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .output()
        .expect("Python 3 is a build-time prerequisite for danoni-importer");
    if !result.status.success() {
        panic!(
            "DanOni importer preparation failed:\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let text = String::from_utf8(result.stdout).expect("Bundle receipt UTF-8");
    let rows: Vec<_> = text.lines().collect();
    assert!(
        rows.len() == 2
            && rows
                .iter()
                .all(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())),
        "Malformed bundle preparation receipt"
    );
    println!("cargo:rustc-env=DDI_DANONI_BUNDLE_ID={}", rows[0]);
    println!(
        "cargo:rustc-env=DDI_DANONI_BUNDLE_MANIFEST_SHA256={}",
        rows[1]
    );
}

fn copy_file(source: &std::path::Path, destination: &std::path::Path) {
    let bytes = std::fs::read(source).expect("Read bundled asset");
    if std::fs::read(destination).is_ok_and(|existing| existing == bytes) {
        return;
    }
    std::fs::create_dir_all(destination.parent().unwrap()).expect("Asset directory");
    std::fs::write(destination, bytes).expect("Stage bundled asset");
}

fn copy_assets(source: &std::path::Path, destination: &std::path::Path) {
    for entry in std::fs::read_dir(source).expect("Bundled assets").flatten() {
        let path = entry.path();
        let kind = entry.file_type().expect("Asset file type");
        assert!(!kind.is_symlink(), "Bundled assets must be ordinary files");
        if kind.is_dir() {
            copy_assets(&path, &destination.join(entry.file_name()));
        } else if kind.is_file() {
            copy_file(&path, &destination.join(entry.file_name()));
        }
    }
}
