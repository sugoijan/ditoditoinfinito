//! The parts that need no helper or movie, and the host against helpers
//! that misbehave (shell scripts standing in for the real one, on Unix);
//! `tests/video.rs` decodes generated clips through the real helper.

use super::*;

#[test]
fn names_and_checksums() {
    assert_eq!(extension_of("Bad Apple!!.AVI"), ".avi");
    assert_eq!(extension_of("bga.mp4"), ".mp4");
    assert_eq!(extension_of("noext"), "");
    assert_eq!(extension_of("weird.ext with space"), "");
    assert_eq!(extension_of("x.verylongext"), "");
    // zlib's check value.
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    assert_eq!(crc32(b""), 0);
}

fn format() -> VideoFormat {
    VideoFormat {
        container: ddi_platform::video::VideoContainer::Unknown,
        codec: ddi_platform::video::VideoCodec::Unknown,
    }
}

/// Events of `v` until one matches `done` (or 20 s).
fn events_until(
    v: &mut impl VideoDecoder,
    mut done: impl FnMut(&VideoEvent) -> bool,
) -> Vec<VideoEvent> {
    let mut all = Vec::new();
    let start = Instant::now();
    loop {
        let mut events = Vec::new();
        v.poll(&mut events);
        let stop = events.iter().any(&mut done);
        all.extend(events);
        if stop || start.elapsed().as_secs() > 20 {
            return all;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn error_of(events: &[VideoEvent]) -> Option<&str> {
    events.iter().find_map(|e| match e {
        VideoEvent::Error(m) => Some(m.as_str()),
        _ => None,
    })
}

#[test]
fn without_the_helper_movies_fail_as_on_the_web() {
    let mut host = VideoHost::with_helper(None);
    let mut v = host.open(
        PathBuf::from("/nowhere/movie.avi").into(),
        "movie.avi",
        &format(),
    );
    v.want(4);
    let events = events_until(&mut v, |e| matches!(e, VideoEvent::Error(_)));
    assert_eq!(events, [VideoEvent::Error(NOT_INCLUDED.into())]);
    assert_eq!(v.pending(), 0);
    assert_eq!(host.status(), HelperStatus::Failed(NOT_INCLUDED.into()));
    let id = host.probe(&PathBuf::from("/nowhere/movie.avi").into(), "movie.avi");
    let start = Instant::now();
    let answer = loop {
        if let Some(a) = host.probe_result(id) {
            break a;
        }
        assert!(start.elapsed().as_secs() < 10);
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(answer, Err(NOT_INCLUDED.into()));
    // A missing file named as the helper cannot start.
    let mut host = VideoHost::with_helper(Some(PathBuf::from("/nowhere/ddi-video-helper")));
    let mut v = host.open(
        PathBuf::from("/nowhere/movie.avi").into(),
        "movie.avi",
        &format(),
    );
    let events = events_until(&mut v, |e| matches!(e, VideoEvent::Error(_)));
    let e = error_of(&events).unwrap();
    assert!(e.contains("cannot start the video decoder"), "{e}");
    assert!(matches!(host.status(), HelperStatus::Failed(_)));
}

#[test]
fn reads_go_through_a_window() {
    let dir = std::env::temp_dir().join(format!("ddi-video-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("f.bin");
    let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &data).unwrap();
    let mut f = helper::OpenFile::open(&MovieFile {
        path: path.clone(),
        range: None,
    })
    .unwrap();
    assert_eq!(f.size(), data.len() as u64);
    assert_eq!(f.read_at(10, 5).unwrap(), &data[10..15]);
    // Inside the window: no read.
    assert_eq!(f.read_at(60_000, 4).unwrap(), &data[60_000..60_004]);
    // Past the end: short, then empty.
    assert_eq!(f.read_at(199_998, 10).unwrap(), &data[199_998..]);
    assert_eq!(f.read_at(200_000, 10).unwrap(), &[] as &[u8]);
    assert_eq!(f.read_at(u64::MAX - 1, 10).unwrap(), &[] as &[u8]);
    // A range: a stored zip entry's bytes.
    let mut f = helper::OpenFile::open(&MovieFile {
        path,
        range: Some((100, 50)),
    })
    .unwrap();
    assert_eq!(f.size(), 50);
    assert_eq!(f.read_at(0, 100).unwrap(), &data[100..150]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_helper_is_found_beside_the_executable() {
    let root = std::env::temp_dir().join(format!("ddi-video-find-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let name = format!("{HELPER_NAME}{}", std::env::consts::EXE_SUFFIX);
    // A build's profile folder, from its executable or a test's.
    let staged = root.join("ws/target/debug/video").join(&name);
    std::fs::create_dir_all(staged.parent().unwrap()).unwrap();
    std::fs::write(&staged, b"#!").unwrap();
    for exe in [
        "ws/target/debug/ditoditoinfinito",
        "ws/target/debug/deps/video-0123",
        "ws/target/debug/examples/screenshot",
    ] {
        assert_eq!(helper_near(&root.join(exe)), Some(staged.clone()), "{exe}");
    }
    // Another profile's is not borrowed, nor anything above `target`.
    assert_eq!(
        helper_near(&root.join("ws/target/desktop/ditoditoinfinito")),
        None
    );
    let above = root.join("ws/video").join(&name);
    std::fs::create_dir_all(above.parent().unwrap()).unwrap();
    std::fs::write(&above, b"#!").unwrap();
    assert_eq!(
        helper_near(&root.join("ws/target/desktop/ditoditoinfinito")),
        None
    );
    // A package: beside the executable.
    let packaged = root.join("app/video").join(&name);
    std::fs::create_dir_all(packaged.parent().unwrap()).unwrap();
    std::fs::write(&packaged, b"#!").unwrap();
    assert_eq!(
        helper_near(&root.join("app/ditoditoinfinito")),
        Some(packaged)
    );
    assert_eq!(helper_near(&root.join("bin/ditoditoinfinito")), None);
    // A macOS app: in its helpers.
    let bundled = root.join("Game.app/Contents/Helpers/video").join(&name);
    std::fs::create_dir_all(bundled.parent().unwrap()).unwrap();
    std::fs::write(&bundled, b"#!").unwrap();
    assert_eq!(
        helper_near(&root.join("Game.app/Contents/MacOS/ditoditoinfinito")),
        Some(bundled)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn frames_are_checked_against_their_slot() {
    let layout = ddi_video_protocol::Layout::new();
    assert_eq!(
        planes_in_slot(&layout, 0, 640, 480).ok(),
        Some(640 * 480 * 3 / 2)
    );
    assert!(planes_in_slot(&layout, 2, 640, 480).is_err());
    assert!(planes_in_slot(&layout, 0, 0, 480).is_err());
    assert!(planes_in_slot(&layout, 0, 100_000, 100_000).is_err());
    assert!(planes_in_slot(&layout, 1, 3840, 2160).is_ok());
    assert!(planes_in_slot(&layout, 1, 8_294_400, 1).is_err());
}

/// A shell script standing in for the helper: it writes `frames` (replies
/// already encoded with their sequence numbers) to its stdout as soon as
/// it starts, then does `then`.
#[cfg(unix)]
fn stand_in(dir: &Path, name: &str, frames: &[(u32, Reply)], then: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let mut bytes = Vec::new();
    for (seq, reply) in frames {
        ddi_video_protocol::write_frame(&mut bytes, *seq, &reply.encode().unwrap()).unwrap();
    }
    let escaped: String = bytes.iter().map(|b| format!("\\{b:03o}")).collect();
    let script = format!("#!/bin/sh\nprintf '{escaped}'\n{then}\n");
    let path = dir.join(name);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
fn hello() -> Reply {
    Reply::Hello {
        version: ddi_video_protocol::VERSION,
        decoder: "stand-in".into(),
        sandbox: "none".into(),
        threads: 1,
    }
}

#[cfg(unix)]
#[test]
fn a_helper_that_dies_hangs_or_lies_fails_the_movie_not_the_host() {
    let dir = std::env::temp_dir().join(format!("ddi-video-hostile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let movie = dir.join("m.avi");
    std::fs::write(&movie, b"not a movie").unwrap();
    let deadline = std::time::Duration::from_millis(500);
    let open = |host: &mut VideoHost| -> (String, HelperStatus) {
        let mut v = host.open(movie.clone().into(), "m.avi", &format());
        v.want(1);
        let events = events_until(&mut v, |e| matches!(e, VideoEvent::Error(_)));
        (
            error_of(&events).unwrap_or("no error").to_string(),
            host.status(),
        )
    };
    // Exits right after its Hello: the open finds it dead; the next open
    // starts a fresh one, which says Hello again.
    let dead = stand_in(&dir, "dead.sh", &[(1, hello())], "exit 7");
    let mut host = VideoHost::with_deadline(Some(dead), deadline);
    for _ in 0..2 {
        let (e, status) = open(&mut host);
        assert!(e.starts_with("the video decoder crashed"), "{e}");
        assert!(matches!(status, HelperStatus::Ready { .. }), "{status:?}");
    }
    // Never answers: the deadline, then the host goes on.
    let hang = stand_in(&dir, "hang.sh", &[(1, hello())], "sleep 30");
    let mut host = VideoHost::with_deadline(Some(hang), deadline);
    let start = Instant::now();
    let (e, _) = open(&mut host);
    assert!(e.contains("a call took longer than"), "{e}");
    assert!(start.elapsed().as_secs() < 10);
    // Says no Hello: cannot start.
    let mute = stand_in(&dir, "mute.sh", &[], "sleep 30");
    let mut host = VideoHost::with_deadline(Some(mute), deadline);
    let (e, status) = open(&mut host);
    assert!(e.contains("does not start"), "{e}");
    assert!(matches!(status, HelperStatus::Failed(_)), "{status:?}");
    // Refuses to serve (exits as a helper whose sandbox does not hold).
    let unsafe_helper = stand_in(
        &dir,
        "open.sh",
        &[],
        "echo 'the sandbox does not hold' >&2; exit 3",
    );
    let mut host = VideoHost::with_deadline(Some(unsafe_helper), deadline);
    let (e, _) = open(&mut host);
    assert!(e.contains("does not start"), "{e}");
    // Answers a frame larger than its slot: refused, the helper killed.
    let opened = Reply::Opened {
        id: 1,
        width: 320,
        height: 240,
        duration: 1.0,
        codec: "stand-in".into(),
    };
    let huge = Reply::Frame {
        id: 1,
        slot: 1,
        pts: 0.0,
        width: 100_000,
        height: 100_000,
        colorspace: 601,
        full_range: false,
        decode_us: 1,
    };
    let liar = stand_in(
        &dir,
        "liar.sh",
        &[(1, hello()), (2, opened.clone()), (3, huge)],
        "sleep 30",
    );
    let mut host = VideoHost::with_deadline(Some(liar), deadline);
    let mut v = host.open(movie.clone().into(), "m.avi", &format());
    let events = events_until(&mut v, |e| matches!(e, VideoEvent::Opened(_)));
    assert!(
        events.iter().any(|e| matches!(e, VideoEvent::Opened(_))),
        "{events:?}"
    );
    v.want(1);
    let events = events_until(&mut v, |e| matches!(e, VideoEvent::Error(_)));
    let e = error_of(&events).unwrap();
    assert!(e.contains("outside its slot"), "{e}");
    // Floods: replies out of sequence are a crash, and the flood is not
    // read into memory (the reader's channel is bounded).
    let flood = stand_in(
        &dir,
        "flood.sh",
        &[(1, hello())],
        "while :; do printf '\\010\\000\\000\\000\\001\\000\\000\\000\\007\\001\\000\\000\\000\\000\\000\\000'; done",
    );
    let mut host = VideoHost::with_deadline(Some(flood), deadline);
    let (e, _) = open(&mut host);
    assert!(
        e.contains("out of sequence") || e.contains("crashed"),
        "{e}"
    );
    // Garbage: malformed frames are a crash too.
    let garbage = stand_in(
        &dir,
        "garbage.sh",
        &[],
        "head -c 100000 /dev/urandom; sleep 30",
    );
    let mut host = VideoHost::with_deadline(Some(garbage), deadline);
    let (e, status) = open(&mut host);
    assert!(e.contains("does not start"), "{e}");
    assert!(matches!(status, HelperStatus::Failed(_)));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn coincident_frames_seek_to_the_last_picture_at_that_time() {
    let times = [0.0, 0.0, 0.1, 0.2, 0.2, 0.2, 0.3];
    for (target, expected) in [(0.0, 1), (0.05, 1), (0.1, 2), (0.2, 5), (0.25, 5), (0.3, 6)] {
        assert_eq!(frame_showing_at(&times, target), expected);
    }
    assert_eq!(frame_showing_at(&[0.1, 0.2], 0.0), 0);
}

#[test]
fn recovery_keeps_the_undelivered_part_of_a_shared_timestamp() {
    let mut r = Resume {
        time: 1.0,
        remaining: 1,
    };
    assert!(r.skip(0.9));
    assert!(r.skip(1.0));
    assert!(!r.skip(1.0));
    let mut r = Resume {
        time: 0.0,
        remaining: 2,
    };
    assert!(r.skip(0.0));
    assert!(r.skip(0.0));
    assert!(!r.skip(0.0));
    let mut r = Resume {
        time: 1.0,
        remaining: 1,
    };
    assert!(r.skip(1.0));
    assert!(!r.skip(1.0 + 5e-7));
    assert!(!r.skip(1.1));
}

#[test]
fn native_selection_requires_an_explicit_windows_opt_in() {
    for os in ["macos", "windows", "linux"] {
        assert!(!Backend::Software.native_on(os));
        assert_eq!(Backend::Auto.native_on(os), os == "macos");
        assert_eq!(Backend::VideoToolbox.native_on(os), os == "macos");
        for backend in [Backend::MediaFoundation, Backend::MediaFoundationAuto] {
            assert_eq!(backend.native_on(os), os == "windows");
            assert_eq!(backend.unavailable_on(os), os != "windows");
        }
    }
    assert!(Backend::MediaFoundation.strict());
    assert!(!Backend::MediaFoundationAuto.strict());
    assert_eq!(
        Backend::parse("mediafoundation"),
        Some(Backend::MediaFoundation)
    );
    assert_eq!(
        Backend::parse("mediafoundation-auto"),
        Some(Backend::MediaFoundationAuto)
    );
}
