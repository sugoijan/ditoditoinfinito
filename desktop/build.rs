//! Build-time preparation only: the bundled assets staged beside the
//! executable, the video decoder helper (`video/helper`, feature `video`)
//! built and staged in `video/` beside it, and the optional DanOni
//! importer. Neither FFmpeg nor the SDK is linked into the desktop binary.
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
    if env::var_os("CARGO_FEATURE_VIDEO").is_some() {
        video_helper(&root, profile, &os);
    }
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

/// Builds `ddi-video-helper` (its static FFmpeg first, through its own
/// build script) in a target directory of its own with the `helper`
/// profile, and stages it with its notice and FFmpeg's description in
/// `video/` beside the executable, where `ddi_desktop::video::helper_path`
/// finds it. Skipped, with a warning and a game that plays songs with their
/// still background, when `DDI_NO_VIDEO_HELPER=1`, when cross-compiling,
/// or on Windows unless `DDI_VIDEO_NATIVE=1` (its FFmpeg build needs MSYS2
/// and the MSVC tools; `video/build-native.sh`).
fn video_helper(root: &std::path::Path, profile: &std::path::Path, os: &str) {
    for name in [
        "DDI_NO_VIDEO_HELPER",
        "DDI_VIDEO_NATIVE",
        "DDI_FFMPEG_NATIVE",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    for path in [
        "video/helper/src",
        "video/helper/build.rs",
        "video/helper/Cargo.toml",
        "video/protocol/src",
        "video/ddivideo.c",
        "video/packets.c",
        "video/ddi_io.c",
        "video/ddi_io.h",
        "video/FFMPEG.toml",
        "video/build-native.sh",
        "video/lib.sh",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    let skip = |why: &str| {
        println!(
            "cargo:warning=no video decoder helper: {why}; movies show their song's background (DDI_VIDEO_HELPER names one at run time)"
        );
    };
    if env::var("DDI_NO_VIDEO_HELPER").as_deref() == Ok("1") {
        return skip("DDI_NO_VIDEO_HELPER=1");
    }
    if env::var("HOST") != env::var("TARGET") {
        return skip("cross-compiling (the helper is built on its own host)");
    }
    if os == "windows" && env::var("DDI_VIDEO_NATIVE").as_deref() != Ok("1") {
        return skip("Windows builds it only with DDI_VIDEO_NATIVE=1 (MSYS2 and the MSVC tools)");
    }
    let target_dir = root.join("target/video/helper-target");
    let cargo = env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from);
    let result = Command::new(cargo)
        .args([
            "build",
            "-p",
            "ddi-video-helper",
            "--profile",
            "helper",
            "--target-dir",
        ])
        .arg(&target_dir)
        .current_dir(root)
        // Nested Cargo builds must never share the parent's jobserver or target lock.
        .env_remove("CARGO_MAKEFLAGS")
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("RUSTC_WRAPPER")
        .output()
        .expect("cargo runs the helper's build");
    if !result.status.success() {
        panic!(
            "building the video decoder helper failed (DDI_NO_VIDEO_HELPER=1 builds without it):
{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let exe = format!(
        "ddi-video-helper{}",
        if os == "windows" { ".exe" } else { "" }
    );
    let built = target_dir.join("helper").join(&exe);
    let video = profile.join("video");
    copy_file(&built, &video.join(&exe));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(video.join(&exe), std::fs::Permissions::from_mode(0o755));
    }
    let native = match env::var_os("DDI_FFMPEG_NATIVE") {
        Some(p) => PathBuf::from(p),
        None => root
            .join("target/video/native")
            .join(env::var("TARGET").unwrap()),
    };
    for (from, to) in [
        ("ffmpeg.json", "ffmpeg.json"),
        (
            "ddivideo-native.LICENSE.txt",
            "ddi-video-helper.LICENSE.txt",
        ),
    ] {
        let source = native.join(from);
        if source.is_file() {
            copy_file(&source, &video.join(to));
        }
    }
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
