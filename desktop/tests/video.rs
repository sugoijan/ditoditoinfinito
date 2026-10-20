//! The FFmpeg helper (feature `video`) on clips generated here with the
//! `ffmpeg` command (FFmpeg's test pattern, written to a temporary
//! folder), against `ffmpeg`'s own decode: every frame, the frame times,
//! exact seeks (into a gap of dropped frames too), loops, a truncated and
//! a corrupt file, two movies at once, and a second host starting a helper
//! of its own. Skipped without the helper (built and staged by
//! `desktop/build.rs`) or without `ffmpeg` with libx264.
#![cfg(feature = "video")]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ddi_desktop::video::{self, HelperStatus, VideoHost};
use ddi_platform::video::{
    VideoBackend, VideoCodec, VideoContainer, VideoDecoder, VideoEvent, VideoFormat,
};

fn ffmpeg(args: &[&str]) -> bool {
    std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

/// A two-second clip of `size`, encoded with `encode`, and `ffmpeg`'s
/// decode of it as raw YUV 4:2:0 (as `video/clips.sh` makes them).
fn clip(dir: &Path, name: &str, size: &str, encode: &[&str]) -> Option<(PathBuf, PathBuf)> {
    let out = dir.join(name);
    let src = format!("testsrc2=size={size}:rate=30");
    let mut args = vec!["-f", "lavfi", "-i", &src, "-t", "2"];
    args.extend_from_slice(encode);
    let o = out.to_str()?.to_string();
    args.push(&o);
    if !ffmpeg(&args) {
        return None;
    }
    let reference = dir.join(format!("{name}.yuv"));
    let r = reference.to_str()?.to_string();
    let ok = ffmpeg(&[
        "-i",
        &o,
        "-fps_mode",
        "passthrough",
        "-sws_flags",
        "area+accurate_rnd+bitexact",
        "-sws_dither",
        "none",
        "-pix_fmt",
        "yuv420p",
        "-f",
        "rawvideo",
        &r,
    ]);
    ok.then_some((out, reference))
}

fn format() -> VideoFormat {
    VideoFormat {
        container: VideoContainer::Unknown,
        codec: VideoCodec::Unknown,
    }
}

/// Events until one matches `done` (or a minute).
fn until(v: &mut impl VideoDecoder, mut done: impl FnMut(&VideoEvent) -> bool) -> Vec<VideoEvent> {
    let mut all = Vec::new();
    let start = Instant::now();
    loop {
        let mut events = Vec::new();
        v.poll(&mut events);
        let stop = events.iter().any(&mut done);
        all.extend(events);
        if stop || start.elapsed() > Duration::from_secs(60) {
            return all;
        }
        std::thread::sleep(Duration::from_micros(200));
    }
}

#[test]
fn clips_decode_as_ffmpeg_decodes_them() {
    let Some(module) = video::helper_path() else {
        eprintln!("no video helper: skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("ddi-video-clips-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Movies extracted from zips go to this run's own cache folder.
    // SAFETY: set before any thread of this test binary reads it (one
    // test here).
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    let clips = [
        (
            "xvid.avi",
            "320x240",
            &["-c:v", "mpeg4", "-vtag", "XVID", "-bf", "2", "-q:v", "4"][..],
            60.0,
        ),
        (
            "xvid-gap.avi",
            "320x240",
            &[
                "-vf",
                "select='not(between(n,10,19))'",
                "-fps_mode",
                "passthrough",
                "-c:v",
                "mpeg4",
                "-vtag",
                "XVID",
                "-bf",
                "2",
                "-q:v",
                "4",
            ][..],
            60.0,
        ),
        (
            "mpeg2.avi",
            "320x240",
            &[
                "-c:v",
                "mpeg2video",
                "-bf",
                "2",
                "-q:v",
                "4",
                "-f",
                "mpeg2video",
            ][..],
            60.0,
        ),
        (
            "h264.mp4",
            "320x240",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p"][..],
            99.0,
        ),
    ];
    let mut made = Vec::new();
    for (name, size, encode, psnr) in clips {
        match clip(&dir, name, size, encode) {
            Some(c) => made.push((name, c, psnr)),
            None => eprintln!("{name}: no ffmpeg (with this encoder): skipped"),
        }
    }
    if made.is_empty() {
        std::fs::remove_dir_all(&dir).ok();
        return;
    }
    let mut host = VideoHost::with_helper(Some(module.clone()));
    for (name, (path, reference), min_psnr) in &made {
        let r = video::check(&mut host, path, Some(reference));
        for line in r.lines(name) {
            eprintln!("{line}");
        }
        assert!(r.failures.is_empty(), "{name}: {:?}", r.failures);
        let (psnr, refs) = r.psnr.unwrap();
        assert_eq!(refs, r.frames.len(), "{name}");
        assert!(psnr.iter().all(|p| p >= min_psnr), "{name}: {psnr:?}");
        let expected = if *name == "xvid-gap.avi" { 50 } else { 60 };
        assert_eq!(r.frames.len(), expected, "{name}");
    }
    // A new host starts a helper of its own, sandboxed.
    let mut again = VideoHost::with_helper(Some(module));
    let (_, (path, _), _) = &made[0];
    let r = video::check(&mut again, path, None);
    assert!(r.failures.is_empty());
    match &r.status {
        Some(HelperStatus::Ready {
            decoder, sandbox, ..
        }) => {
            assert!(decoder.starts_with("FFmpeg "), "{decoder}");
            assert_ne!(sandbox, "none");
        }
        other => panic!("{other:?}"),
    }
    // Two movies at once, interleaved: each gets its own frames, in
    // order from its start.
    let mut a = host.open(path.clone().into(), "a.avi", &format());
    let mut b = host.open(made.last().unwrap().1.0.clone().into(), "b.mp4", &format());
    for _ in 0..2 {
        a.want(3);
        b.want(3);
    }
    let frames = |v: &mut ddi_desktop::video::DesktopVideo| -> Vec<f64> {
        let mut pts = Vec::new();
        let start = Instant::now();
        while pts.len() < 6 && start.elapsed() < Duration::from_secs(30) {
            let mut e = Vec::new();
            v.poll(&mut e);
            pts.extend(e.into_iter().filter_map(|e| match e {
                VideoEvent::Frame(f) => Some(f.pts),
                _ => None,
            }));
            std::thread::sleep(Duration::from_millis(1));
        }
        pts
    };
    let (pa, pb) = (frames(&mut a), frames(&mut b));
    assert_eq!(pa.len(), 6);
    assert_eq!(pb.len(), 6);
    assert!(pa.windows(2).all(|w| w[1] > w[0]) && pb.windows(2).all(|w| w[1] > w[0]));
    assert_eq!(a.pending(), 0);
    // A truncated file plays what it has and ends; a corrupt one fails to
    // open; neither takes the decoder down.
    let bytes = std::fs::read(path).unwrap();
    let cut = dir.join("cut.avi");
    std::fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    let mut c = host.open(cut.into(), "cut.avi", &format());
    c.want(200);
    let events = until(&mut c, |e| {
        matches!(e, VideoEvent::End { .. } | VideoEvent::Error(_))
    });
    let frames = events
        .iter()
        .filter(|e| matches!(e, VideoEvent::Frame(_)))
        .count();
    assert!(
        frames > 10 && frames < 60,
        "{frames} frames from half a file"
    );
    let junk = dir.join("junk.avi");
    std::fs::write(
        &junk,
        (0..100_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut j = host.open(junk.into(), "junk.avi", &format());
    let events = until(&mut j, |e| matches!(e, VideoEvent::Error(_)));
    assert!(
        matches!(events.last(), Some(VideoEvent::Error(m)) if m.contains("cannot open")),
        "{events:?}"
    );
    let r = video::check(&mut host, path, None);
    assert!(
        r.failures.is_empty(),
        "after the corrupt file: {:?}",
        r.failures
    );
    drop((a, b, c, j));
    // A movie in a zip, read in place when stored, extracted once into
    // the cache when compressed: the same frames as the file's.
    let first = |host: &mut VideoHost, file: video::MovieFile| -> Vec<(f64, Vec<u8>)> {
        let mut v = host.open(file, "z.avi", &format());
        v.want(6);
        let mut out = Vec::new();
        let start = Instant::now();
        while out.len() < 6 && start.elapsed() < Duration::from_secs(30) {
            let mut e = Vec::new();
            v.poll(&mut e);
            out.extend(e.into_iter().filter_map(|e| match e {
                VideoEvent::Frame(f) => Some((f.pts, f.planes.clone())),
                _ => None,
            }));
            std::thread::sleep(Duration::from_millis(1));
        }
        out
    };
    let direct = first(&mut host, path.clone().into());
    assert_eq!(direct.len(), 6);
    let pack = dir.join("pack");
    std::fs::create_dir_all(pack.join("Song")).unwrap();
    std::fs::copy(path, pack.join("Song/movie.avi")).unwrap();
    for (zip_name, level) in [("stored.zip", "-0"), ("deflated.zip", "-9")] {
        let zipped = dir.join(zip_name);
        let ok = std::process::Command::new("zip")
            .current_dir(&pack)
            .args([
                "-q",
                "-X",
                level,
                zipped.to_str().unwrap(),
                "Song/movie.avi",
            ])
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            eprintln!("no zip command: the zip part skipped");
            break;
        }
        let reader = ddi_desktop::library::files::ZipReader::open(&zipped, &|_| None).unwrap();
        let files = ddi_desktop::play::load::Files::Zip {
            zip: zipped.clone(),
            reader: std::sync::Arc::new(reader),
            dir: "Song".into(),
        };
        let (file, range) = files.movie("movie.avi").unwrap();
        assert_eq!(range.is_some(), level == "-0", "{zip_name}");
        if range.is_none() {
            assert!(file.starts_with(dir.join("home")), "{}", file.display());
        }
        let got = first(&mut host, video::MovieFile { path: file, range });
        assert_eq!(got, direct, "{zip_name}");
    }
    std::fs::remove_dir_all(&dir).ok();
}
