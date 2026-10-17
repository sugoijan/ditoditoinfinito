//! Streaming VideoToolbox H.264 against the sandboxed decoder. Generated
//! movies only, no window or sound. Hardware refusal can skip on CI Macs.
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
    Command::new("ffmpeg")
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
            "-g",
            "30",
        ])
        .args(args)
        .arg(&path)
        .status()
        .is_ok_and(|s| s.success())
        .then_some(path)
}
fn format() -> VideoFormat {
    VideoFormat {
        container: VideoContainer::Unknown,
        codec: VideoCodec::Unknown,
    }
}
fn events(v: &mut impl VideoDecoder) -> Vec<VideoEvent> {
    let began = Instant::now();
    loop {
        let mut out = Vec::new();
        v.poll(&mut out);
        if !out.is_empty() {
            return out;
        }
        assert!(began.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_micros(200));
    }
}
fn first(host: &mut VideoHost, source: MovieFile, name: &str) -> (f64, Vec<u8>) {
    let mut v = host.open(source, name, &format());
    assert!(matches!(events(&mut v)[0], VideoEvent::Opened(_)));
    v.want(1);
    match events(&mut v).remove(0) {
        VideoEvent::Frame(f) => {
            let f = video::copy_to_cpu(f).unwrap();
            (f.pts, f.planes)
        }
        other => panic!("{other:?}"),
    }
}
#[test]
fn streaming_h264_has_the_modules_frames_and_exact_seeks() {
    let Some(module) = video::module_path() else {
        eprintln!("no video module: skipped");
        return;
    };
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/scratch/native-packet-tests");
    std::fs::create_dir_all(&dir).unwrap();
    // One test in this binary, before any decoder thread starts.
    unsafe {
        std::env::set_var("DDI_HOME", dir.join("home"));
        std::env::set_var("DDI_VIDEO_CACHE", dir.join("cache"));
    }
    let Some(basic) = encode(&dir, "basic.mp4", &["-bf", "3"]) else {
        eprintln!("no ffmpeg/libx264: skipped");
        return;
    };
    let mut native = VideoHost::with_backend(Some(module.clone()), Backend::VideoToolbox);
    let mut wasm = VideoHost::with_module(Some(module.clone()));
    let trial = video::check(&mut native, &basic, None);
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
        eprintln!("hardware unavailable: {:?}", trial.failures);
        return;
    }
    let mut paths = vec![basic.clone()];
    for (name, args) in [
        ("no-b.mp4", vec!["-bf", "0"]),
        ("basic.mkv", vec!["-bf", "3"]),
        ("no-b.mkv", vec!["-bf", "0"]),
        ("fractional.mp4", vec!["-r", "30000/1001", "-bf", "2"]),
        ("start.mp4", vec!["-bf", "3", "-output_ts_offset", "5"]),
        ("start.mkv", vec!["-bf", "3", "-output_ts_offset", "5"]),
        (
            "vfr.mp4",
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
            "vfr.mkv",
            vec![
                "-vf",
                "select='not(between(n,10,19))'",
                "-fps_mode",
                "passthrough",
                "-bf",
                "3",
            ],
        ),
        ("long-gop.mp4", vec!["-t", "6", "-g", "180", "-bf", "3"]),
        (
            "hd.mp4",
            vec!["-vf", "scale=1280:720", "-t", "1", "-bf", "3"],
        ),
        (
            "range.mp4",
            vec![
                "-color_range",
                "pc",
                "-colorspace",
                "bt709",
                "-x264-params",
                "fullrange=on:colormatrix=bt709",
            ],
        ),
    ] {
        paths.push(encode(&dir, name, &args).unwrap());
    }
    for (name, audio_codec) in [("audio.mp4", "aac"), ("audio.mkv", "libopus")] {
        let path = dir.join(name);
        assert!(
            Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(&basic)
                .args([
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440",
                    "-t",
                    "2",
                    "-map",
                    "0:v:0",
                    "-map",
                    "1:a:0",
                    "-c:v",
                    "copy",
                    "-c:a",
                    audio_codec
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        paths.push(path);
    }
    let mut auto = VideoHost::with_backend(Some(module.clone()), Backend::Auto);
    for path in &paths {
        let expected = video::check(&mut wasm, path, None);
        assert!(
            expected.failures.is_empty(),
            "{} module: {:?}",
            path.display(),
            expected.failures
        );
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
            "{} exact timestamps and planes",
            path.display()
        );
        assert_eq!(
            got.info,
            expected.info,
            "{} stream metadata",
            path.display()
        );
        assert_eq!(got.decoder.unwrap().backend, "VideoToolbox (hardware)");
        let selected = video::check(&mut auto, path, None);
        assert!(selected.failures.is_empty(), "{:?}", selected.failures);
        assert_eq!(selected.frames, expected.frames);
        assert_eq!(selected.decoder.unwrap().backend, "VideoToolbox (hardware)");
        eprintln!(
            "{}: {} frames, {} seeks per backend match",
            path.file_name().unwrap().to_string_lossy(),
            got.frames.len(),
            got.seeks
        );
    }
    // Range-relative reads and two independent sandboxes/OS sessions.
    let bytes = std::fs::read(&basic).unwrap();
    let envelope = dir.join("stored.bin");
    let mut wrapped = vec![0x5a; 137];
    wrapped.extend_from_slice(&bytes);
    wrapped.extend_from_slice(&[0xa5; 83]);
    std::fs::write(&envelope, wrapped).unwrap();
    assert_eq!(
        first(&mut native, basic.clone().into(), "basic.mp4"),
        first(
            &mut native,
            MovieFile {
                path: envelope,
                range: Some((137, bytes.len() as u64))
            },
            "stored.mp4"
        )
    );
    let mkv = dir.join("basic.mkv");
    let expected_a = first(&mut wasm, basic.clone().into(), "a.mp4");
    let expected_b = first(&mut wasm, mkv.clone().into(), "b.mkv");
    let mut a = native.open(basic.clone().into(), "a.mp4", &format());
    let mut b = native.open(mkv.into(), "b.mkv", &format());
    assert!(matches!(events(&mut a)[0], VideoEvent::Opened(_)));
    assert!(matches!(events(&mut b)[0], VideoEvent::Opened(_)));
    a.want(1);
    b.want(1);
    for (v, expected) in [(&mut a, expected_a), (&mut b, expected_b)] {
        match events(v).remove(0) {
            VideoEvent::Frame(f) => {
                let f = video::copy_to_cpu(f).unwrap();
                assert_eq!((f.pts, f.planes), expected);
            }
            other => panic!("{other:?}"),
        }
    }
    a.seek(1.0);
    a.want(1);
    b.want(1);
    assert!(matches!(&events(&mut a)[0],VideoEvent::Frame(f) if (f.pts-1.0).abs()<1e-6));
    assert!(matches!(&events(&mut b)[0],VideoEvent::Frame(f) if f.pts>0.0 && f.pts<0.1));
    drop(a);
    drop(b);
    // Rapid seeks discard superseded generations.
    let mut v = native.open(basic.clone().into(), "rapid.mp4", &format());
    assert!(matches!(events(&mut v)[0], VideoEvent::Opened(_)));
    v.want(16);
    v.seek(1.5);
    v.want(8);
    v.seek(0.5);
    v.want(1);
    let out = events(&mut v);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(matches!(&out[0],VideoEvent::Frame(f) if (f.pts-0.5).abs()<1e-6));
    drop(v);
    // Unsupported slice layout/depth and open-GOP seek refusals use the module.
    for (name, args) in [
        ("slices.mp4", vec!["-x264-params", "slices=2"]),
        ("deep.mp4", vec!["-pix_fmt", "yuv420p10le"]),
    ] {
        let path = encode(&dir, name, &args).unwrap();
        let expected = video::check(&mut wasm, &path, None);
        let got = video::check(&mut auto, &path, None);
        assert!(
            expected.failures.is_empty() && got.failures.is_empty(),
            "{name}: {:?} / {:?}",
            expected.failures,
            got.failures
        );
        assert_eq!(got.frames, expected.frames);
        assert_eq!(got.decoder.unwrap().backend, "module");
    }
    // A later in-band SPS differs only in level. Both containers continue
    // with the module at the last delivered time, without gaps/duplicates.
    for name in ["changed.mp4", "changed.mkv"] {
        let path = encode(
            &dir,
            name,
            &["-x264-params", "repeat-headers=1:keyint=30", "-bf", "3"],
        )
        .unwrap();
        let mut reader =
            video::packets::PacketReader::open(Some(module.clone()), path.clone().into(), name)
                .unwrap();
        let configuration = reader.configuration().unwrap();
        let framing = ddi_library::h264::stream(&configuration, &[])
            .unwrap()
            .framing;
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
            let nal = ddi_library::h264::nal_units(&packet.data, framing)
                .into_iter()
                .find(|n| n[0] & 31 == 7)
                .unwrap();
            let within = nal.as_ptr() as usize - packet.data.as_ptr() as usize;
            let base = usize::try_from(packet.position).unwrap();
            let end = (base + packet.data.len() + 64).min(bytes.len());
            let start = bytes[base..end]
                .windows(packet.data.len())
                .position(|data| data == packet.data)
                .expect("compressed packet near its container position");
            bytes[base + start + within + 3] = 31;
            break;
        }
        assert!(keys >= 2);
        drop(reader);
        std::fs::write(&path, bytes).unwrap();
        let expected = video::check(&mut wasm, &path, None);
        let got = video::check(&mut auto, &path, None);
        assert!(
            expected.failures.is_empty() && got.failures.is_empty(),
            "{name}: {:?} / {:?}",
            expected.failures,
            got.failures
        );
        assert_eq!(
            got.frames, expected.frames,
            "{name} fallback keeps the complete timeline"
        );
        assert!(
            got.decoder
                .unwrap()
                .fallback
                .unwrap()
                .contains("parameter sets changed")
        );
    }
    // An open GOP can seek to a non-IDR key packet. Refuse that seek and
    // recover through the module; no broadening from sequential success.
    let open = encode(
        &dir,
        "open-gop.mp4",
        &[
            "-t",
            "4",
            "-x264-params",
            "open-gop=1:keyint=30:min-keyint=30:scenecut=0",
            "-bf",
            "3",
        ],
    )
    .unwrap();
    let expected = video::check(&mut wasm, &open, None);
    let got = video::check(&mut auto, &open, None);
    assert!(
        expected.failures.is_empty() && got.failures.is_empty(),
        "open GOP: {:?} / {:?}",
        expected.failures,
        got.failures
    );
    assert_eq!(got.frames, expected.frames);
    assert_eq!(got.decoder.unwrap().backend, "module");
    // Malformed native framing and out-of-range stored members are refused.
    let broken = dir.join("broken.mp4");
    std::fs::write(&broken, b"\0\0\0\x18ftypisom broken").unwrap();
    let mut bad = native.open(broken.into(), "broken.mp4", &format());
    assert!(matches!(events(&mut bad)[0], VideoEvent::Error(_)));
    drop(bad);
    let mut bad = native.open(
        MovieFile {
            path: basic.clone(),
            range: Some((u64::MAX, 20)),
        },
        "range.mp4",
        &format(),
    );
    assert!(matches!(events(&mut bad)[0], VideoEvent::Error(_)));
    drop(bad);
}
