//! The production VideoToolbox path against the sandboxed module, on
//! generated movies only. No window, audio device or third-party fixtures.
#![cfg(all(feature = "video", target_os = "macos"))]

use ddi_desktop::video::{self, Backend, MovieFile, VideoHost};
use ddi_platform::video::{
    VideoBackend, VideoCodec, VideoContainer, VideoDecoder, VideoEvent, VideoFormat,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

fn encode(dir: &Path, name: &str, args: &[&str]) -> Option<PathBuf> {
    let path = dir.join(name);
    let result = Command::new("ffmpeg")
        .args([
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
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .args(args)
        .arg(&path)
        .status();
    result.is_ok_and(|s| s.success()).then_some(path)
}
fn format() -> VideoFormat {
    VideoFormat {
        container: VideoContainer::Unknown,
        codec: VideoCodec::Unknown,
    }
}
fn event(v: &mut impl VideoDecoder) -> Vec<VideoEvent> {
    let start = Instant::now();
    loop {
        let mut events = Vec::new();
        v.poll(&mut events);
        if !events.is_empty() {
            return events;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "decoder did not answer"
        );
        std::thread::sleep(Duration::from_micros(200));
    }
}
fn first(host: &mut VideoHost, file: MovieFile) -> (Vec<u8>, f64) {
    let mut v = host.open(file, "range.avi", &format());
    assert!(matches!(event(&mut v).first(), Some(VideoEvent::Opened(_))));
    v.want(1);
    match event(&mut v).remove(0) {
        VideoEvent::Frame(f) => {
            let f = video::copy_to_cpu(f).unwrap();
            (f.planes, f.pts)
        }
        e => panic!("{e:?}"),
    }
}

#[test]
fn native_avi_matches_the_module_and_falls_back_without_changing_position() {
    // CI's macOS runners are virtual machines whose VideoToolbox stalls
    // (DDI_SKIP_VT=1 there); these comparisons run on a Mac.
    if std::env::var_os("DDI_SKIP_VT").is_some() {
        eprintln!("DDI_SKIP_VT: skipped");
        return;
    }
    let Some(module) = video::helper_path() else {
        eprintln!("no video helper: skipped");
        return;
    };
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/video-native-tests");
    std::fs::create_dir_all(&dir).unwrap();
    // This binary has one test; no settings or cache from the maintainer.
    unsafe {
        std::env::set_var("DDI_HOME", dir.join("home"));
    }
    let Some(basic) = encode(&dir, "basic.avi", &["-g", "30", "-bf", "3"]) else {
        eprintln!("no ffmpeg/libx264: skipped");
        return;
    };
    let mut native = VideoHost::with_backend(None, Backend::VideoToolbox);
    let mut wasm = VideoHost::with_helper(Some(module.clone()));
    // A missing/unavailable hardware session is a skip on old/virtual Macs,
    // but the auto fallback below must still pass. On the development Mac,
    // DDI_REQUIRE_VT=1 makes a skip a failure.
    let trial = video::check(&mut native, &basic, None);
    let hardware = trial.failures.is_empty();
    if !hardware {
        assert!(
            std::env::var_os("DDI_REQUIRE_VT").is_none(),
            "hardware VT required: {:?}",
            trial.failures
        );
        assert!(
            trial
                .failures
                .iter()
                .all(|e| e.contains("VideoToolbox session")
                    || e.contains("hardware property")
                    || e.contains("did not use hardware")),
            "native regression is not a hardware skip: {:?}",
            trial.failures
        );
        eprintln!("hardware VT unavailable: {:?}", trial.failures);
    }
    let mut paths = vec![basic.clone()];
    paths.push(
        encode(
            &dir,
            "hd.avi",
            &["-vf", "scale=1280:720", "-t", "1", "-bf", "3"],
        )
        .unwrap(),
    );
    for (name, args) in [
        ("no-b.avi", vec!["-bf", "0", "-g", "30"]),
        ("long-gop.avi", vec!["-t", "6", "-g", "180", "-bf", "3"]),
        (
            "gap.avi",
            vec![
                "-vf",
                "select='not(between(n,10,19))'",
                "-fps_mode",
                "passthrough",
                "-bf",
                "3",
            ],
        ),
        (
            "full-range.avi",
            vec![
                "-color_range",
                "pc",
                "-colorspace",
                "bt709",
                "-x264-params",
                "fullrange=on:colormatrix=bt709",
            ],
        ),
        ("fractional.avi", vec!["-r", "30000/1001", "-bf", "2"]),
    ] {
        paths.push(encode(&dir, name, &args).expect("generated H.264 fixture"));
    }
    // An AVI stream whose first DTS is nonzero, still using the same index.
    let mut start = std::fs::read(&basic).unwrap();
    let at = start.windows(4).position(|b| b == b"strh").unwrap() + 8 + 28;
    start[at..at + 4].copy_from_slice(&5u32.to_le_bytes());
    let nonzero = dir.join("nonzero.avi");
    std::fs::write(&nonzero, start).unwrap();
    paths.push(nonzero);
    for path in &paths {
        let expected = video::check(&mut wasm, path, None);
        assert!(
            expected.failures.is_empty(),
            "{} module: {:?}",
            path.display(),
            expected.failures
        );
        if hardware {
            let got = video::check(&mut native, path, None);
            assert!(
                got.failures.is_empty(),
                "{} VT: {:?}",
                path.display(),
                got.failures
            );
            assert_eq!(
                got.frames,
                expected.frames,
                "{} timestamps and planes",
                path.display()
            );
            assert_eq!(got.decoder.unwrap().backend, "VideoToolbox (hardware)");
            assert_eq!(got.info, expected.info, "{} stream info", path.display());
            assert_eq!(got.seeks, expected.seeks);
            eprintln!(
                "{}: {} frames and {} seeks match the module",
                path.file_name().unwrap().to_string_lossy(),
                got.frames.len(),
                got.seeks
            );
        }
    }
    let mut auto = VideoHost::with_backend(Some(module.clone()), Backend::Auto);
    // Auto must fall back on streams outside the gate.
    let mp4 = encode(&dir, "fallback.mp4", &["-pix_fmt", "yuv420p10le"]).unwrap();
    let expected = video::check(&mut wasm, &mp4, None);
    let got = video::check(&mut auto, &mp4, None);
    assert!(got.failures.is_empty(), "{:?}", got.failures);
    assert_eq!(got.frames, expected.frames);
    let status = got.decoder.unwrap();
    assert_eq!(status.backend, "software");
    assert!(status.fallback.is_some());
    // Unsupported depth and malformed/truncated sources must leave the native
    // gate. Auto still gets the module's frames for a supported 10-bit clip.
    if let Some(deep) = encode(&dir, "depth10.avi", &["-pix_fmt", "yuv420p10le"]) {
        let expected = video::check(&mut wasm, &deep, None);
        let got = video::check(&mut auto, &deep, None);
        assert!(expected.failures.is_empty() && got.failures.is_empty());
        assert_eq!(got.frames, expected.frames);
        assert_eq!(got.decoder.unwrap().backend, "software");
    }
    let cut = dir.join("truncated.avi");
    let bytes = std::fs::read(&basic).unwrap();
    std::fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    let mut v = native.open(cut.into(), "cut.avi", &format());
    assert!(matches!(event(&mut v).first(), Some(VideoEvent::Error(_))));
    drop(v);
    if hardware {
        // Range-relative offsets: bytes outside this stored-zip-like range
        // must never become part of AVI header, index or packet reads.
        let bytes = std::fs::read(&basic).unwrap();
        let mut range = vec![0x5a; 123];
        range.extend_from_slice(&bytes);
        range.extend_from_slice(&[0xa5; 117]);
        let ranged = dir.join("stored.bin");
        std::fs::write(&ranged, range).unwrap();
        assert_eq!(
            first(&mut native, basic.clone().into()),
            first(
                &mut native,
                MovieFile {
                    path: ranged,
                    range: Some((123, bytes.len() as u64))
                }
            )
        );
        // A later SPS differs only in its declared level, keeping the same
        // pixels. The narrow native gate must refuse it, then the module
        // must resume after the last emitted frame without duplicates/gaps.
        let mut changed = bytes.clone();
        let header = ddi_library::avi::parse_header(
            &changed[..changed.len().min(ddi_library::avi::HEADER_LEN)],
        )
        .unwrap();
        let chunks = header.parse_idx1(&changed[header.idx1_at() as usize..]);
        let c = chunks.iter().skip(1).find(|c| c.key).unwrap();
        let at = c.offset as usize;
        let packet = &changed[at..at + c.size as usize];
        let nal = ddi_library::h264::nal_units(packet, ddi_library::h264::Framing::AnnexB)
            .into_iter()
            .find(|n| n[0] & 31 == 7)
            .unwrap();
        let offset = nal.as_ptr() as usize - packet.as_ptr() as usize;
        changed[at + offset + 3] = 31;
        let path = dir.join("changed-sps.avi");
        std::fs::write(&path, changed).unwrap();
        let expected = video::check(&mut wasm, &path, None);
        let got = video::check(&mut auto, &path, None);
        assert!(expected.failures.is_empty(), "{:?}", expected.failures);
        assert!(got.failures.is_empty(), "{:?}", got.failures);
        assert_eq!(got.frames, expected.frames);
        assert!(
            got.decoder
                .unwrap()
                .fallback
                .unwrap()
                .contains("parameter sets changed")
        );
        // Multiple seeks supersede both unpolled and in-flight frames.
        let mut v = native.open(basic.clone().into(), "rapid.avi", &format());
        assert!(matches!(event(&mut v).first(), Some(VideoEvent::Opened(_))));
        v.want(16);
        v.seek(1.5);
        v.want(8);
        v.seek(0.5);
        v.want(1);
        let events = event(&mut v);
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(matches!(&events[0], VideoEvent::Frame(f) if (f.pts - 0.5).abs() < 1e-6));
    }
}
