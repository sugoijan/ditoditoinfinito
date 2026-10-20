//! Generated streaming HEVC/VP9 against the module, without a window/audio.
#![cfg(all(feature = "video", target_os = "macos"))]
use ddi_desktop::video::{self, Backend, MovieFile, VideoHost};
use ddi_platform::video::{
    VideoBackend, VideoCodec, VideoContainer, VideoDecoder, VideoEvent, VideoFormat,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn encode(dir: &Path, name: &str, codec: &str, args: &[&str]) -> Option<PathBuf> {
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
    if codec == "libx265" {
        cmd.args([
            "-x265-params",
            "pools=1:frame-threads=1:log-level=error:open-gop=0",
        ]);
    } else {
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
    cmd.args(args)
        .arg(&path)
        .status()
        .is_ok_and(|s| s.success())
        .then_some(path)
}
#[test]
fn hevc_and_vp9_stream_frames_and_exact_seeks_match_the_module() {
    let Some(module) = video::helper_path() else {
        eprintln!("no video helper: skipped");
        return;
    };
    let Ok(encoders) = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
    else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    assert!(
        encoders.status.success(),
        "ffmpeg could not list its encoders"
    );
    let encoders = String::from_utf8_lossy(&encoders.stdout);
    let available = encoders
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect::<Vec<_>>();
    if !available.contains(&"libx265") || !available.contains(&"libvpx-vp9") {
        eprintln!("no libx265/libvpx-vp9 for the combined codec suite: skipped");
        return;
    }
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/native-codec-tests");
    std::fs::create_dir_all(&dir).unwrap();
    unsafe {
        std::env::set_var("DDI_HOME", dir.join("home"));
    }
    let mut hardware = VideoHost::with_backend(Some(module.clone()), Backend::VideoToolbox);
    let mut baseline = VideoHost::with_helper(Some(module.clone()));
    let mut auto = VideoHost::with_backend(Some(module.clone()), Backend::Auto);
    let mut both_hardware = true;
    for (codec, ext) in [("libx265", "mp4"), ("libvpx-vp9", "webm")] {
        let basic = encode(&dir, &format!("{codec}.{ext}"), codec, &[])
            .expect("available encoder must generate the basic codec fixture");
        let trial = video::check(&mut hardware, &basic, None);
        if !trial.failures.is_empty() {
            assert!(
                std::env::var_os("DDI_REQUIRE_VT").is_none(),
                "hardware required: {:?}",
                trial.failures
            );
            assert!(
                trial
                    .failures
                    .iter()
                    .all(|e| e.contains("VideoToolbox session")
                        || e.contains("hardware property")
                        || e.contains("did not use hardware")),
                "not a hardware refusal: {:?}",
                trial.failures
            );
            eprintln!("{codec} hardware unavailable: {:?}", trial.failures);
            both_hardware = false;
            continue;
        }
        let mut paths = vec![basic];
        for (suffix, args) in [
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
            paths.push(encode(&dir, &format!("{codec}-{suffix}.{ext}"), codec, &args).unwrap());
        }
        if codec == "libx265" {
            paths.push(encode(&dir, "hevc.mkv", codec, &[]).unwrap());
            paths.push(
                encode(&dir, "hevc-poc-wrap.mp4", codec, &["-t", "10", "-g", "360"]).unwrap(),
            );
            paths.push(encode(&dir, "hevc-no-b.mp4", codec, &["-bf", "0"]).unwrap());
        } else {
            paths.push(encode(&dir, "vp9.mp4", codec, &[]).unwrap());
            paths.push(encode(&dir, "vp9-lossless.webm", codec, &["-lossless", "1"]).unwrap());
        }
        for path in paths {
            let expected = video::check(&mut baseline, &path, None);
            let got = video::check(&mut hardware, &path, None);
            assert!(
                expected.failures.is_empty(),
                "{} module: {:?}",
                path.display(),
                expected.failures
            );
            assert!(
                got.failures.is_empty(),
                "{} native: {:?}",
                path.display(),
                got.failures
            );
            assert_eq!(
                got.frames,
                expected.frames,
                "{} exact timestamps/planes",
                path.display()
            );
            assert_eq!(got.info, expected.info, "{} metadata", path.display());
            assert_eq!(got.decoder.unwrap().backend, "VideoToolbox (hardware)");
            let selected = video::check(&mut auto, &path, None);
            assert!(selected.failures.is_empty(), "{:?}", selected.failures);
            assert_eq!(selected.frames, expected.frames);
            assert_eq!(selected.decoder.unwrap().backend, "VideoToolbox (hardware)");
            eprintln!(
                "{}: {} frames, {} exact seeks per backend",
                path.file_name().unwrap().to_string_lossy(),
                got.frames.len(),
                got.seeks
            );
        }
        let deep = encode(
            &dir,
            &format!("{codec}-10bit.{ext}"),
            codec,
            &["-pix_fmt", "yuv420p10le"],
        )
        .unwrap();
        let expected = video::check(&mut baseline, &deep, None);
        let got = video::check(&mut auto, &deep, None);
        assert!(
            expected.failures.is_empty() && got.failures.is_empty(),
            "10bit: {:?} / {:?}",
            expected.failures,
            got.failures
        );
        assert_eq!(got.frames, expected.frames);
        assert_eq!(got.decoder.unwrap().backend, "software");
    }

    if !both_hardware {
        eprintln!("combined HEVC/VP9 cases need both hardware decoders: skipped");
        return;
    }
    let hevc = dir.join("libx265.mp4");
    let vp9 = dir.join("libvpx-vp9.webm");
    let expected_a = first_frame(&mut baseline, &hevc, None);
    let expected_b = first_frame(&mut baseline, &vp9, None);
    let expected_seek = first_frame(&mut baseline, &hevc, Some(0.5));
    let data = std::fs::read(&hevc).unwrap();
    let wrapped = dir.join("stored.bin");
    let mut bytes = vec![0x5a; 137];
    bytes.extend_from_slice(&data);
    bytes.extend_from_slice(&[0xa5; 83]);
    std::fs::write(&wrapped, bytes).unwrap();
    let mut a = hardware.open(
        MovieFile {
            path: wrapped,
            range: Some((137, data.len() as u64)),
        },
        "stored.mp4",
        &unknown_format(),
    );
    let mut b = hardware.open(vp9.clone().into(), "second.webm", &unknown_format());
    for v in [&mut a, &mut b] {
        assert!(matches!(events(v).remove(0), VideoEvent::Opened(_)));
    }
    assert_eq!(read_frame(&mut a), expected_a);
    assert_eq!(read_frame(&mut b), expected_b);
    a.want(16);
    a.seek(1.5);
    a.want(8);
    a.seek(0.5);
    assert_eq!(read_frame(&mut a), expected_seek);
    assert!(read_frame(&mut b).0 > 0.0);
    drop(a);
    drop(b);
    for name in ["changed.mp4", "changed.mkv"] {
        let path = encode(
            &dir,
            name,
            "libx265",
            &[
                "-x265-params",
                "pools=1:frame-threads=1:log-level=error:open-gop=0:repeat-headers=1",
            ],
        )
        .unwrap();
        let mut reader =
            video::packets::PacketReader::open(Some(module.clone()), path.clone().into(), name)
                .unwrap();
        let config = reader.configuration().unwrap();
        let width = usize::from((config[21] & 3) + 1);
        let mut bytes = std::fs::read(&path).unwrap();
        let mut keys = 0;
        while let Some(packet) = reader.next_packet().unwrap() {
            if !packet.key() {
                continue;
            }
            keys += 1;
            if keys < 2 {
                continue;
            }
            let mut at = 0;
            while at < packet.data.len() {
                let len = packet.data[at..at + width]
                    .iter()
                    .fold(0usize, |v, b| (v << 8) | usize::from(*b));
                at += width;
                let nal = &packet.data[at..at + len];
                if (nal[0] >> 1) & 63 == 33 {
                    // SPS general_level_idc: RBSP byte 12, after the
                    // SPS's first byte and the profile/compatibility flags.
                    let (mut rbsp, mut zeros) = (0, 0);
                    let mut offset = None;
                    for (i, &b) in nal[2..].iter().enumerate() {
                        if zeros >= 2 && b == 3 {
                            zeros = 0;
                            continue;
                        }
                        if rbsp == 12 {
                            offset = Some(at + 2 + i);
                            break;
                        }
                        zeros = if b == 0 { zeros + 1 } else { 0 };
                        rbsp += 1;
                    }
                    let base = usize::try_from(packet.position).unwrap();
                    let end = (base + packet.data.len() + 64).min(bytes.len());
                    let start = bytes[base..end]
                        .windows(packet.data.len())
                        .position(|b| b == packet.data)
                        .unwrap();
                    bytes[base + start + offset.unwrap()] ^= 1;
                    break;
                }
                at += len;
            }
            assert!(at < packet.data.len(), "fixture has a repeated SPS");
            break;
        }
        assert!(keys >= 2);
        drop(reader);
        std::fs::write(&path, bytes).unwrap();
        let got = compare_auto(&mut auto, &mut baseline, &path);
        assert!(
            got.decoder
                .unwrap()
                .fallback
                .unwrap()
                .contains("parameter sets changed")
        );
    }
    let open = encode(
        &dir,
        "open-gop.mp4",
        "libx265",
        &[
            "-t",
            "4",
            "-x265-params",
            "pools=1:frame-threads=1:log-level=error:open-gop=1:keyint=30:min-keyint=30:scenecut=0",
        ],
    )
    .unwrap();
    assert_eq!(
        compare_auto(&mut auto, &mut baseline, &open)
            .decoder
            .unwrap()
            .backend,
        "software"
    );
    let slices = encode(
        &dir,
        "slices.mp4",
        "libx265",
        &[
            "-vf",
            "scale=256:256",
            "-x265-params",
            "pools=1:frame-threads=1:log-level=error:open-gop=0:slices=2",
        ],
    )
    .unwrap();
    assert_eq!(
        compare_auto(&mut auto, &mut baseline, &slices)
            .decoder
            .unwrap()
            .backend,
        "software"
    );
    // Two-pass VP9 creates hidden alternate references/superframes. They
    // are outside this class and must recover through the module.
    let alt = dir.join("altref.webm");
    for pass in ["1", "2"] {
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
            "libvpx-vp9",
            "-pix_fmt",
            "yuv420p",
            "-b:v",
            "100k",
            "-cpu-used",
            "4",
            "-auto-alt-ref",
            "1",
            "-lag-in-frames",
            "25",
            "-pass",
            pass,
            "-passlogfile",
        ])
        .arg(dir.join("vp9-pass"));
        if pass == "1" {
            cmd.args(["-f", "null", "/dev/null"]);
        } else {
            cmd.arg(&alt);
        }
        assert!(cmd.status().unwrap().success());
    }
    assert_eq!(
        compare_auto(&mut auto, &mut baseline, &alt)
            .decoder
            .unwrap()
            .backend,
        "software"
    );
}

fn compare_auto(auto: &mut VideoHost, baseline: &mut VideoHost, path: &Path) -> video::CheckReport {
    let expected = video::check(baseline, path, None);
    let got = video::check(auto, path, None);
    assert!(
        expected.failures.is_empty() && got.failures.is_empty(),
        "{}: {:?} / {:?}",
        path.display(),
        expected.failures,
        got.failures
    );
    assert_eq!(
        got.frames,
        expected.frames,
        "{} fallback timeline",
        path.display()
    );
    got
}

fn unknown_format() -> VideoFormat {
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
        assert!(start.elapsed() < std::time::Duration::from_secs(20));
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
}
fn read_frame(v: &mut impl VideoDecoder) -> (f64, Vec<u8>) {
    v.want(1);
    let mut out = events(v);
    assert_eq!(out.len(), 1);
    match out.remove(0) {
        VideoEvent::Frame(f) => {
            let f = video::copy_to_cpu(f).unwrap();
            (f.pts, f.planes)
        }
        other => panic!("{other:?}"),
    }
}
fn first_frame(host: &mut VideoHost, path: &Path, target: Option<f64>) -> (f64, Vec<u8>) {
    let mut v = host.open(path.to_path_buf().into(), "movie.mp4", &unknown_format());
    let opened = events(&mut v);
    assert!(
        matches!(opened.first(), Some(VideoEvent::Opened(_))),
        "{}: {opened:?}",
        path.display()
    );
    if let Some(t) = target {
        v.seek(t);
    }
    read_frame(&mut v)
}
