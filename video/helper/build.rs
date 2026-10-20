//! Compiles the FFmpeg shims (`video/ddivideo.c`, `video/packets.c`,
//! `video/ddi_io.c`) natively and links the static FFmpeg that
//! `video/build-native.sh` leaves in `target/video/native/<target>/`
//! (running the script when that is missing or stale). Nothing here is
//! linked into the game: `crates/desktop/build.rs` builds this binary separately.
use std::{env, path::PathBuf, process::Command};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let target = env::var("TARGET").unwrap();
    for file in [
        "video/ddivideo.c",
        "video/packets.c",
        "video/ddi_io.c",
        "video/ddi_io.h",
        "video/FFMPEG.toml",
        "video/build-native.sh",
        "video/lib.sh",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(file).display());
    }
    println!("cargo:rerun-if-env-changed=DDI_FFMPEG_NATIVE");
    println!("cargo:rerun-if-env-changed=DDI_VIDEO_HELPER_CHECK_ONLY");
    // `cargo check`/`clippy` for another target (Windows from a Mac): no
    // FFmpeg to build or link; the binary is never produced.
    if env::var("DDI_VIDEO_HELPER_CHECK_ONLY").as_deref() == Ok("1") {
        println!("cargo:rustc-env=DDI_FFMPEG_VERSION=unbuilt");
        return;
    }
    let prefix = match env::var_os("DDI_FFMPEG_NATIVE") {
        Some(p) => PathBuf::from(p),
        None => root.join("target/video/native").join(&target),
    };
    let description = prefix.join("ffmpeg.json");
    println!("cargo:rerun-if-changed={}", description.display());
    let current = Command::new("bash")
        .arg(root.join("video/build-native.sh"))
        .args(["--target", &target, "--check"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !current && env::var_os("DDI_FFMPEG_NATIVE").is_none() {
        println!(
            "cargo:warning=building FFmpeg natively for {target} (video/build-native.sh; a few minutes the first time)"
        );
        let status = Command::new("bash")
            .arg(root.join("video/build-native.sh"))
            .args(["--target", &target])
            .status()
            .expect("bash runs video/build-native.sh");
        assert!(
            status.success(),
            "video/build-native.sh failed; the helper needs its FFmpeg (see the script's log in target/video/native/)"
        );
    }
    let json = std::fs::read_to_string(&description)
        .unwrap_or_else(|e| panic!("{}: {e} (run video/build-native.sh)", description.display()));
    let version = json
        .lines()
        .find_map(|l| l.trim().strip_prefix("\"version\": \""))
        .and_then(|v| v.strip_suffix("\","))
        .expect("ffmpeg.json names FFmpeg's version");
    println!("cargo:rustc-env=DDI_FFMPEG_VERSION={version}");

    let mut build = cc::Build::new();
    build
        .files([
            root.join("video/ddivideo.c"),
            root.join("video/packets.c"),
            root.join("video/ddi_io.c"),
        ])
        .include(prefix.join("include"))
        .include(root.join("video"))
        .define("DDI_NATIVE", "1")
        .opt_level(2)
        .warnings(true);
    if !target.contains("msvc") {
        // The compiler's default dialect: strict C11 would hide `strdup`.
        build.flag("-fvisibility=hidden");
    }
    build.compile("ddishim");
    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    // In dependency order. MSVC's linker takes the archives by path (they
    // keep their `lib*.a` names there); the others find them by name.
    for lib in ["avformat", "avcodec", "swscale", "avutil", "z"] {
        if target.contains("msvc") {
            println!(
                "cargo:rustc-link-arg={}",
                prefix.join("lib").join(format!("lib{lib}.a")).display()
            );
        } else {
            println!("cargo:rustc-link-lib=static={lib}");
        }
    }
    if target.contains("linux") {
        println!("cargo:rustc-link-lib=m");
        println!("cargo:rustc-link-lib=pthread");
    }
}
