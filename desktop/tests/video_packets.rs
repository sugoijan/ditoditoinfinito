//! Demux exports against ffprobe on generated media. No window or sound.
#![cfg(feature = "video")]
use ddi_desktop::video::{
    self, MovieFile,
    packets::{INDEX_PAGE, PacketReader},
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn dump(s: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in s.lines().filter(|s| !s.is_empty()) {
        let hex = line
            .split_once(':')
            .unwrap()
            .1
            .trim_start()
            .split("  ")
            .next()
            .unwrap()
            .replace(' ', "");
        for at in (0..hex.len()).step_by(2) {
            bytes.push(u8::from_str_radix(&hex[at..at + 2], 16).unwrap());
        }
    }
    bytes
}
fn number(v: &Value, key: &str) -> Option<i64> {
    v[key]
        .as_i64()
        .or_else(|| v[key].as_str().and_then(|s| s.parse().ok()))
}
fn reference(path: &Path) -> Value {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_streams",
            "-show_packets",
            "-show_data",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn packets_configuration_and_seeks_match_ffprobe() {
    let Some(module) = video::module_path() else {
        eprintln!("no video module: skipped");
        return;
    };
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/scratch/video-packet-tests");
    std::fs::create_dir_all(&dir).unwrap();
    // One test in this binary, before any decoder thread starts.
    unsafe {
        std::env::set_var("DDI_HOME", dir.join("home"));
        std::env::set_var("DDI_VIDEO_CACHE", dir.join("cache"));
    }
    let Ok(encoders) = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
    else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let encoders = String::from_utf8_lossy(&encoders.stdout);
    let available = encoders
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect::<Vec<_>>();
    if !available.contains(&"libopus") || !available.contains(&"libx264") {
        eprintln!("no libx264/libopus for the generated clips: skipped");
        return;
    }
    let cases: &[(&str, &[&str])] = &[
        ("h264.mp4", &["-c:v", "libx264", "-bf", "3", "-g", "12"]),
        (
            "h264.avi",
            &["-c:v", "libx264", "-bf", "3", "-c:a", "pcm_s16le"],
        ),
        (
            "vfr.mkv",
            &[
                "-vf",
                "select=not(between(n\\,8\\,13))",
                "-fps_mode",
                "passthrough",
                "-c:v",
                "libx264",
            ],
        ),
        ("start.mp4", &["-c:v", "libx264", "-output_ts_offset", "5"]),
        (
            "hevc.mp4",
            &[
                "-c:v",
                "libx265",
                "-x265-params",
                "log-level=error:pools=1",
                "-tag:v",
                "hvc1",
            ],
        ),
        (
            "vp9.webm",
            &[
                "-c:v",
                "libvpx-vp9",
                "-deadline",
                "realtime",
                "-cpu-used",
                "8",
            ],
        ),
        // AV1 is not a built-in decoder in our module. Demux must still work.
        (
            "av1.mkv",
            &[
                "-c:v",
                "libsvtav1",
                "-preset",
                "12",
                "-svtav1-params",
                "lp=1",
            ],
        ),
    ];
    for (name, codec) in cases {
        let encoder = codec.windows(2).find(|w| w[0] == "-c:v").unwrap()[1];
        if !available.contains(&encoder) {
            eprintln!("{name}: no {encoder}, skipped");
            continue;
        }
        let path = dir.join(name);
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=128x96:rate=30000/1001",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440",
                "-t",
                "1",
                "-c:a",
                "libopus",
            ])
            .args(*codec)
            .args(["-pix_fmt", "yuv420p"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "encode {name}");
        let reference = reference(&path);
        let stream = &reference["streams"][0];
        let mut reader =
            PacketReader::open(Some(module.clone()), path.clone().into(), name).unwrap();
        assert_eq!(reader.info().codec, stream["codec_name"]);
        let rational = stream["time_base"]
            .as_str()
            .unwrap()
            .split('/')
            .map(|n| n.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(reader.info().time_base.as_slice(), rational);
        assert_eq!(reader.info().start, number(stream, "start_pts"));
        let configuration = reader.configuration().unwrap();
        if *name == "h264.avi" {
            // No container extradata: ffprobe's optional extract_extradata
            // bitstream filter synthesizes it, our demux-only build does not.
            assert!(configuration.is_empty());
        } else {
            assert_eq!(
                configuration,
                stream["extradata"].as_str().map(dump).unwrap_or_default(),
                "{name} configuration"
            );
        }
        let mut packets = Vec::new();
        while let Some(p) = reader.next_packet().unwrap() {
            packets.push(p);
            assert!(packets.len() < 1000);
        }
        assert!(reader.next_packet().unwrap().is_none(), "sticky EOF");
        let expected = reference["packets"].as_array().unwrap();
        assert_eq!(packets.len(), expected.len(), "{name}");
        for (p, e) in packets.iter().zip(expected) {
            assert_eq!(p.pts, number(e, "pts"), "{name} PTS");
            assert_eq!(p.dts, number(e, "dts"), "{name} DTS");
            assert_eq!(
                p.duration,
                number(e, "duration").unwrap_or(0),
                "{name} duration"
            );
            assert_eq!(
                p.position,
                number(e, "pos").unwrap_or(-1),
                "{name} position"
            );
            assert_eq!(
                p.key(),
                e["flags"].as_str().unwrap().contains('K'),
                "{name} key"
            );
            assert_eq!(
                p.data,
                dump(e["data"].as_str().unwrap()),
                "{name} compressed bytes"
            );
        }
        let count = reader.index_count().unwrap();
        let mut index = Vec::new();
        for first in (0..count).step_by(INDEX_PAGE as usize) {
            index.extend(reader.index(first, INDEX_PAGE).unwrap());
        }
        assert_eq!(index.len(), count as usize);
        assert!(reader.index(count, INDEX_PAGE).unwrap().is_empty());
        assert!(reader.index(0, INDEX_PAGE + 1).is_err());
        for (epoch, target) in [packets.len() / 2, 0, packets.len() - 1, 2]
            .into_iter()
            .enumerate()
        {
            let ts = packets[target].pts.or(packets[target].dts).unwrap();
            reader.seek(ts).unwrap();
            let p = reader.next_packet().unwrap().unwrap();
            assert_eq!(p.epoch, epoch as u32 + 1);
            assert_eq!(p.sequence, 1);
            assert!(p.key(), "{name}: seek starts at a key packet");
            assert!(
                packets
                    .iter()
                    .any(|old| old.pts == p.pts && old.dts == p.dts && old.data == p.data),
                "{name}: seek packet must match sequential read"
            );
        }
        // Same bytes embedded in a larger file: packet positions stay relative
        // to the member; no read may see its prefix or suffix.
        let bytes = std::fs::read(&path).unwrap();
        let envelope = dir.join("envelope.bin");
        let mut wrapped = vec![0xa5; 137];
        wrapped.extend_from_slice(&bytes);
        wrapped.extend_from_slice(&[0x5a; 83]);
        std::fs::write(&envelope, wrapped).unwrap();
        let mut ranged = PacketReader::open(
            Some(module.clone()),
            MovieFile {
                path: envelope,
                range: Some((137, bytes.len() as u64)),
            },
            name,
        )
        .unwrap();
        // Interleave two private instances, including a seek in the first.
        for expected in packets.iter().take(5) {
            let p = ranged.next_packet().unwrap().unwrap();
            assert_eq!(p.data, expected.data);
            assert_eq!(p.position, expected.position);
            reader.seek(p.pts.or(p.dts).unwrap()).unwrap();
        }
        eprintln!(
            "{name}: {} packets, {count} seek hints, bytes/ticks/configuration/range/seeks match",
            packets.len()
        );
    }
    // A real oversized compressed packet must fail before crossing the ABI.
    // Raw BGR in AVI gives one 9 MiB packet without a codec-dependent encoder.
    let oversized = dir.join("oversized.avi");
    assert!(
        Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=2048x1536:rate=1",
                "-frames:v",
                "1",
                "-c:v",
                "rawvideo",
                "-pix_fmt",
                "bgr24",
                "-threads",
                "1"
            ])
            .arg(&oversized)
            .status()
            .unwrap()
            .success()
    );
    let mut large =
        PacketReader::open(Some(module.clone()), oversized.into(), "oversized.avi").unwrap();
    assert!(large.next_packet().unwrap_err().contains("byte limit"));
    assert!(large.next_packet().unwrap_err().contains("closed"));
    let truncated = dir.join("truncated.mp4");
    let mut bytes = std::fs::read(dir.join("h264.mp4")).unwrap();
    bytes.truncate(bytes.len() / 2);
    std::fs::write(&truncated, bytes).unwrap();
    if let Ok(mut r) = PacketReader::open(Some(module.clone()), truncated.into(), "truncated.mp4") {
        // Some containers tolerate a truncated tail; either EOF or a bounded
        // error is valid, but never an unbounded stream of invented packets.
        for n in 0..100 {
            match r.next_packet() {
                Ok(Some(_)) => assert!(n < 99),
                _ => break,
            }
        }
    }
    let broken = dir.join("broken.mp4");
    std::fs::write(&broken, b"not a container").unwrap();
    assert!(PacketReader::open(Some(module), broken.into(), "broken.mp4").is_err());
}
