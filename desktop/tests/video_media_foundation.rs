//! Deliberate, strict Windows hardware-host validation. Run with --ignored.
//! No window/audio. Missing module, encoders, codecs or DXGI output FAIL.
#![cfg(all(feature = "video", target_os = "windows"))]
use ddi_desktop::video::{self, Backend, MovieFile, VideoHost};
use ddi_platform::video::{
    VideoBackend, VideoCodec, VideoContainer, VideoDecoder, VideoEvent, VideoFormat,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
const SELECTED: &str = "Media Foundation (D3D11; experimental)";

#[test]
#[ignore = "requires a Windows GPU, FFmpeg encoders and the video module"]
fn h264_frames_seeks_and_recovery() {
    suite("libx264", "mp4");
}
#[test]
#[ignore = "requires a Windows GPU, HEVC decoder, FFmpeg encoders and the video module"]
fn hevc_frames_seeks_and_recovery() {
    suite("libx265", "mp4");
}
#[test]
#[ignore = "requires a Windows GPU, VP9 decoder, FFmpeg encoders and the video module"]
fn vp9_frames_seeks_and_recovery() {
    suite("libvpx-vp9", "webm");
}

fn encode(dir: &Path, name: &str, codec: &str, extra: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=128x96:rate=30",
        "-t",
        "2",
        "-c:v",
        codec,
        "-pix_fmt",
        "yuv420p",
        "-g",
        "30",
    ]);
    match codec {
        "libx264" => {
            cmd.args(["-bf", "3", "-x264-params", "open-gop=0"]);
        }
        "libx265" => {
            cmd.args([
                "-x265-params",
                "pools=1:frame-threads=1:log-level=error:open-gop=0",
            ]);
        }
        _ => {
            cmd.args([
                "-deadline",
                "good",
                "-cpu-used",
                "4",
                "-lag-in-frames",
                "0",
                "-auto-alt-ref",
                "0",
            ]);
        }
    }
    assert!(
        cmd.args(extra)
            .arg(&path)
            .status()
            .expect("ffmpeg on PATH")
            .success(),
        "encode {}",
        path.display()
    );
    path
}
fn clean(report: &video::CheckReport, path: &Path, backend: &str) {
    assert!(
        report.failures.is_empty(),
        "{} {backend}: {:?}",
        path.display(),
        report.failures
    );
    assert!(
        !report.frames.is_empty(),
        "{} {backend}: no frames",
        path.display()
    );
    assert_eq!(
        report.decoder.as_ref().unwrap().backend,
        backend,
        "{} selection",
        path.display()
    );
}
fn compare(host: &mut VideoHost, baseline: &mut VideoHost, path: &Path, backend: &str) {
    let expected = video::check(baseline, path, None);
    clean(&expected, path, "software");
    let got = video::check(host, path, None);
    clean(&got, path, backend);
    assert_eq!(got.info, expected.info, "{} metadata", path.display());
    assert_eq!(
        got.frames,
        expected.frames,
        "{} timestamps and I420 CRCs",
        path.display()
    );
    for line in got.lines(&path.file_name().unwrap().to_string_lossy()) {
        eprintln!("{line}");
    }
    assert_eq!(got.seeks, expected.seeks);
    assert!(got.seeks > 0);
    eprintln!(
        "{}: {} frames, {} exact seeks; {}",
        path.display(),
        got.frames.len(),
        got.seeks,
        backend
    );
}
fn suite(codec: &str, ext: &str) {
    let module = video::helper_path().expect("DDI_VIDEO_HELPER must identify the built decoder");
    // Separate test folders; the runner installs DDI_HOME/cache once before
    // starting the process. Tests never mutate process environment in parallel.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/scratch/windows-video")
        .join(codec);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(
        std::env::var_os("DDI_HOME").is_some(),
        "set DDI_HOME to a scratch home (or use video/check-windows.ps1)"
    );
    let mut baseline = VideoHost::with_helper(Some(module.clone()));
    let mut strict = VideoHost::with_backend(Some(module.clone()), Backend::MediaFoundation);
    let mut recover = VideoHost::with_backend(Some(module.clone()), Backend::MediaFoundationAuto);
    let mut default = VideoHost::with_backend(Some(module), Backend::Auto);
    let basic = encode(&dir, &format!("basic.{ext}"), codec, &[]);
    compare(&mut default, &mut baseline, &basic, "software");
    // Exercise availability recovery even on a host where the strict test
    // below will fail. Never turn missing codecs into a successful skip.
    let expected = video::check(&mut baseline, &basic, None);
    clean(&expected, &basic, "software");
    let trial = video::check(&mut recover, &basic, None);
    let selected = trial.decoder.as_ref().expect("decoder selection");
    clean(&trial, &basic, &selected.backend);
    assert_eq!(trial.info, expected.info);
    assert_eq!(trial.frames, expected.frames, "availability recovery");
    if selected.backend == "software" {
        assert!(
            selected.fallback.is_some(),
            "opt-in must report its refusal"
        );
    } else {
        assert_eq!(selected.backend, SELECTED);
    }
    for line in trial.lines("availability/recovery") {
        eprintln!("{line}");
    }

    let mut paths = vec![basic.clone()];
    for (name, args) in [
        ("fractional", vec!["-r", "30000/1001"]),
        ("start", vec!["-output_ts_offset", "5"]),
        (
            "vfr",
            vec![
                "-vf",
                "select='not(between(n,10,19))'",
                "-fps_mode",
                "passthrough",
            ],
        ),
        ("long", vec!["-t", "6", "-g", "180"]),
        ("hd", vec!["-vf", "scale=1280:720", "-t", "1"]),
        ("range", vec!["-color_range", "pc", "-colorspace", "bt709"]),
    ] {
        paths.push(encode(&dir, &format!("{name}.{ext}"), codec, &args));
    }
    paths.push(encode(&dir, "other.mkv", codec, &[]));
    if codec == "libvpx-vp9" {
        paths.push(encode(&dir, "other.mp4", codec, &[]));
        paths.push(encode(&dir, "lossless.webm", codec, &["-lossless", "1"]));
    } else {
        paths.push(encode(&dir, "no-b.mp4", codec, &["-bf", "0"]));
    }
    for path in paths {
        compare(&mut strict, &mut baseline, &path, SELECTED);
        compare(&mut recover, &mut baseline, &path, SELECTED);
    }
    let deep = encode(
        &dir,
        &format!("10bit.{ext}"),
        codec,
        &["-pix_fmt", "yuv420p10le"],
    );
    compare(&mut recover, &mut baseline, &deep, "software");
    let refused = video::check(&mut strict, &deep, None);
    assert!(
        !refused.failures.is_empty(),
        "strict decoder must refuse 10-bit input"
    );
    if codec == "libx264" {
        let avi = encode(&dir, "no-pts.avi", codec, &[]);
        compare(&mut recover, &mut baseline, &avi, "software");
        assert!(!video::check(&mut strict, &avi, None).failures.is_empty());
    }
    if codec == "libx265" {
        let slices = encode(
            &dir,
            "slices.mp4",
            codec,
            &[
                "-vf",
                "scale=256:256",
                "-x265-params",
                "pools=1:frame-threads=1:log-level=error:open-gop=0:slices=2",
            ],
        );
        compare(&mut recover, &mut baseline, &slices, "software");
        let open = encode(
            &dir,
            "open-gop.mp4",
            codec,
            &[
                "-t",
                "4",
                "-x265-params",
                "pools=1:frame-threads=1:log-level=error:open-gop=1:keyint=30:min-keyint=30:scenecut=0",
            ],
        );
        compare(&mut recover, &mut baseline, &open, "software");
    }
    // Stored zip member range, independent simultaneous sessions, cancellation
    // of wants and superseded seeks all go through the real Windows backend.
    let expected = first(&mut baseline, basic.clone().into(), None);
    let expected_seek = first(&mut baseline, basic.clone().into(), Some(0.5));
    let data = std::fs::read(&basic).unwrap();
    let wrapped = dir.join("stored.bin");
    let mut bytes = vec![0x5a; 137];
    bytes.extend_from_slice(&data);
    bytes.extend_from_slice(&[0xa5; 83]);
    std::fs::write(&wrapped, bytes).unwrap();
    let mut a = strict.open(
        MovieFile {
            path: wrapped,
            range: Some((137, data.len() as u64)),
        },
        &format!("stored.{ext}"),
        &unknown(),
    );
    let mut b = strict.open(basic.into(), &format!("other.{ext}"), &unknown());
    for v in [&mut a, &mut b] {
        assert!(matches!(events(v).remove(0), VideoEvent::Opened(_)));
    }
    assert_eq!(frame(&mut a), expected);
    assert_eq!(frame(&mut b), expected);
    a.want(16);
    a.seek(1.5);
    a.want(8);
    a.seek(0.5);
    assert_eq!(frame(&mut a), expected_seek);
    assert!(frame(&mut b).0 > 0.0);
}
fn unknown() -> VideoFormat {
    VideoFormat {
        container: VideoContainer::Unknown,
        codec: VideoCodec::Unknown,
    }
}
fn events(v: &mut impl VideoDecoder) -> Vec<VideoEvent> {
    let start = std::time::Instant::now();
    loop {
        let mut out = Vec::new();
        v.poll(&mut out);
        if !out.is_empty() {
            return out;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(30),
            "decoder event timeout"
        );
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
}
fn frame(v: &mut impl VideoDecoder) -> (f64, Vec<u8>) {
    v.want(1);
    let out = events(v);
    assert_eq!(out.len(), 1);
    match out.into_iter().next().unwrap() {
        VideoEvent::Frame(f) => (f.pts, f.planes),
        other => panic!("{other:?}"),
    }
}
fn first(host: &mut VideoHost, file: MovieFile, seek: Option<f64>) -> (f64, Vec<u8>) {
    let mut v = host.open(file, "movie.mp4", &unknown());
    assert!(matches!(events(&mut v)[0], VideoEvent::Opened(_)));
    if let Some(time) = seek {
        v.seek(time);
    }
    frame(&mut v)
}
