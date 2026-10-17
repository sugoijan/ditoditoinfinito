//! The parts that need no module or movie; `tests/video.rs` decodes
//! generated clips through the module.

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

#[test]
fn without_the_module_movies_fail_as_on_the_web() {
    let mut host = VideoHost::with_module(None);
    let format = VideoFormat {
        container: ddi_platform::video::VideoContainer::Unknown,
        codec: ddi_platform::video::VideoCodec::Unknown,
    };
    let mut v = host.open(
        PathBuf::from("/nowhere/movie.avi").into(),
        "movie.avi",
        &format,
    );
    v.want(4);
    let mut events = Vec::new();
    let start = Instant::now();
    while events.is_empty() && start.elapsed().as_secs() < 10 {
        v.poll(&mut events);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(events, [VideoEvent::Error(NOT_INCLUDED.into())]);
    assert_eq!(v.pending(), 0);
    assert_eq!(host.status(), ModuleStatus::Failed(NOT_INCLUDED.into()));
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
    // A missing file named as the module is the same.
    let mut host = VideoHost::with_module(Some(PathBuf::from("/nowhere/ddivideo.wasm")));
    let mut v = host.open(
        PathBuf::from("/nowhere/movie.avi").into(),
        "movie.avi",
        &format,
    );
    let mut events = Vec::new();
    let start = Instant::now();
    while events.is_empty() && start.elapsed().as_secs() < 10 {
        v.poll(&mut events);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(events, [VideoEvent::Error(NOT_INCLUDED.into())]);
}

#[test]
fn reads_go_through_a_window() {
    let dir = std::env::temp_dir().join(format!("ddi-video-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("f.bin");
    let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &data).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let mut f = OpenFile {
        file,
        base: 0,
        size: data.len() as u64,
        pos: 0,
        cache: Vec::new(),
        cache_at: 0,
    };
    assert_eq!(f.read_at(10, 5).unwrap(), &data[10..15]);
    // Inside the window: no read; past it: a new one.
    assert_eq!(f.cache_at, 10);
    assert_eq!(f.read_at(60_000, 10).unwrap(), &data[60_000..60_010]);
    assert_eq!(f.cache_at, 10);
    assert_eq!(f.read_at(150_000, 100_000).unwrap(), &data[150_000..]);
    assert_eq!(f.read_at(300_000, 10).unwrap(), &[] as &[u8]);
    // A range of the file (a zip's stored entry): its bytes only.
    let mut r = OpenFile {
        file: std::fs::File::open(&path).unwrap(),
        base: 1000,
        size: 5000,
        pos: 0,
        cache: Vec::new(),
        cache_at: 0,
    };
    assert_eq!(r.read_at(0, 4).unwrap(), &data[1000..1004]);
    assert_eq!(r.read_at(4990, 100).unwrap(), &data[5990..6000]);
    let ranged = MovieFile {
        path: path.clone(),
        range: Some((199_000, 5000)),
    };
    assert_eq!(ranged.size(), 1000);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A stand-in for the FFmpeg module, its behaviour chosen by a movie
/// file's first byte: `N` three frames then the end, `T` a trap in
/// `ddi_next`, `L` a loop there that never ends, `F` the movie opened
/// 1000 times over (never closed), `S` 64 MiB of decoder messages with
/// no line end, `B` a picture far larger than its memory (the probe),
/// `P` files outside `/v` asked for. `ddi_width` answers what the mode
/// measured (`F`: the opens that worked; `P`: the first error).
const STAND_IN: &str = r#"
(module
  (import "wasi_snapshot_preview1" "path_open"
    (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_read"
    (func $fd_read (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 2)
  (global $mode (mut i32) (i32.const 0))
  (global $measured (mut i32) (i32.const 0))
  (global $frames (mut i32) (i32.const 0))
  (data (i32.const 256) "../../../etc/passwd")
  (data (i32.const 320) "/etc/passwd")
  (data (i32.const 384) "x\00")
  (func $strlen (param $p i32) (result i32) (local $n i32)
    (block $done (loop $l
      (br_if $done (i32.eqz (i32.load8_u (i32.add (local.get $p) (local.get $n)))))
      (local.set $n (i32.add (local.get $n) (i32.const 1)))
      (br $l)))
    (local.get $n))
  (func $open (param $p i32) (param $len i32) (result i32)
    (call $path_open (i32.const 3) (i32.const 0) (local.get $p) (local.get $len)
      (i32.const 0) (i64.const -1) (i64.const -1) (i32.const 0) (i32.const 64)))
  (func (export "ddi_malloc") (param i32) (result i32) (i32.const 1024))
  (func (export "ddi_free") (param i32))
  (func (export "ddi_open") (param $p i32) (result i32)
    (local $name i32) (local $len i32) (local $i i32)
    ;; past "/v/"
    (local.set $name (i32.add (local.get $p) (i32.const 3)))
    (local.set $len (call $strlen (local.get $name)))
    (if (call $open (local.get $name) (local.get $len)) (then (return (i32.const 0))))
    (i32.store (i32.const 96) (i32.const 128))
    (i32.store (i32.const 100) (i32.const 1))
    (drop (call $fd_read (i32.load (i32.const 64)) (i32.const 96) (i32.const 1) (i32.const 104)))
    (global.set $mode (i32.load8_u (i32.const 128)))
    (global.set $frames (i32.const 0))
    (global.set $measured (i32.const 0))
    (if (i32.eq (global.get $mode) (i32.const 80)) ;; P
      (then
        (global.set $measured (call $open (i32.const 256) (i32.const 19)))
        (if (i32.eqz (call $open (i32.const 320) (i32.const 11)))
          (then (global.set $measured (i32.const 0))))
        (if (i32.eqz (call $path_open (i32.const 0) (i32.const 0) (i32.const 321) (i32.const 10)
              (i32.const 0) (i64.const -1) (i64.const -1) (i32.const 0) (i32.const 64)))
          (then (global.set $measured (i32.const 0))))))
    (if (i32.eq (global.get $mode) (i32.const 70)) ;; F
      (then
        (block $d (loop $l
          (br_if $d (i32.ge_u (local.get $i) (i32.const 1000)))
          (if (i32.eqz (call $open (local.get $name) (local.get $len)))
            (then (global.set $measured (i32.add (global.get $measured) (i32.const 1)))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $l)))))
    (if (i32.eq (global.get $mode) (i32.const 83)) ;; S
      (then
        (drop (memory.grow (i32.const 16)))
        (i32.store (i32.const 96) (i32.const 131072))
        (i32.store (i32.const 100) (i32.const 1048576))
        (block $d (loop $l
          (br_if $d (i32.ge_u (local.get $i) (i32.const 64)))
          (drop (call $fd_write (i32.const 2) (i32.const 96) (i32.const 1) (i32.const 104)))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $l)))))
    (i32.const 16))
  (func (export "ddi_width") (param i32) (result i32)
    (if (result i32) (i32.eq (global.get $mode) (i32.const 66))
      (then (i32.const 100000))
      (else (if (result i32) (i32.or (i32.eq (global.get $mode) (i32.const 70))
                                     (i32.eq (global.get $mode) (i32.const 80)))
        (then (global.get $measured)) (else (i32.const 2))))))
  (func (export "ddi_height") (param i32) (result i32)
    (if (result i32) (i32.eq (global.get $mode) (i32.const 66))
      (then (i32.const 100000)) (else (i32.const 2))))
  (func (export "ddi_codec") (param i32) (result i32) (i32.const 384))
  (func (export "ddi_colorspace") (param i32) (result i32) (i32.const 601))
  (func (export "ddi_full_range") (param i32) (result i32) (i32.const 0))
  (func (export "ddi_planes") (param i32) (result i32) (i32.const 4096))
  (func (export "ddi_planes_size") (param i32) (result i32) (i32.const 6))
  (func (export "ddi_last_time") (param i32) (result f64) (f64.const -1))
  (func (export "ddi_duration") (param i32) (result f64) (f64.const 1))
  (func (export "ddi_next") (param i32) (result f64)
    (if (i32.eq (global.get $mode) (i32.const 84)) (then unreachable))
    (if (i32.eq (global.get $mode) (i32.const 76)) (then (loop $l (br $l))))
    (global.set $frames (i32.add (global.get $frames) (i32.const 1)))
    (if (result f64) (i32.gt_u (global.get $frames) (i32.const 3))
      (then (f64.const -1))
      (else (f64.div (f64.convert_i32_u (global.get $frames)) (f64.const 30)))))
  (func (export "ddi_seek") (param i32 f64) (result i32) (global.set $frames (i32.const 0)) (i32.const 0))
  (func (export "ddi_close") (param i32))
)
"#;

/// A scratch folder with the stand-in module and a movie per mode.
fn stand_in(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ddi-video-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m.wasm"), wat::parse_str(STAND_IN).unwrap()).unwrap();
    for mode in ["N", "T", "L", "F", "S", "B", "P"] {
        std::fs::write(dir.join(format!("{mode}.avi")), mode).unwrap();
    }
    dir
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

fn format() -> VideoFormat {
    VideoFormat {
        container: ddi_platform::video::VideoContainer::Unknown,
        codec: ddi_platform::video::VideoCodec::Unknown,
    }
}

/// Plays the `N` movie to its end: the host still works.
fn plays(host: &mut VideoHost, dir: &Path) {
    let mut v = host.open(dir.join("N.avi").into(), "N.avi", &format());
    v.want(8);
    let events = events_until(&mut v, |e| {
        matches!(e, VideoEvent::End { .. } | VideoEvent::Error(_))
    });
    let frames = events
        .iter()
        .filter(|e| matches!(e, VideoEvent::Frame(_)))
        .count();
    assert_eq!(frames, 3, "{events:?}");
}

fn error_of(events: &[VideoEvent]) -> Option<&str> {
    events.iter().find_map(|e| match e {
        VideoEvent::Error(m) => Some(m.as_str()),
        _ => None,
    })
}

#[test]
fn a_trap_or_a_call_that_never_ends_fails_the_movie_not_the_host() {
    let dir = stand_in("guard");
    let mut host = VideoHost::with_deadline(
        Some(dir.join("m.wasm")),
        std::time::Duration::from_millis(500),
    );
    plays(&mut host, &dir);
    // A trap.
    let mut v = host.open(dir.join("T.avi").into(), "T.avi", &format());
    v.want(2);
    let events = events_until(&mut v, |e| matches!(e, VideoEvent::Error(_)));
    let e = error_of(&events).unwrap();
    assert!(e.starts_with("the video decoder crashed"), "{e}");
    plays(&mut host, &dir);
    // A loop: stopped at the deadline.
    let start = Instant::now();
    let mut v = host.open(dir.join("L.avi").into(), "L.avi", &format());
    v.want(2);
    let events = events_until(&mut v, |e| matches!(e, VideoEvent::Error(_)));
    let e = error_of(&events).unwrap();
    assert!(e.contains("a call took longer than"), "{e}");
    assert!(start.elapsed().as_secs() < 10);
    plays(&mut host, &dir);
    assert!(matches!(host.status(), ModuleStatus::Ready { .. }));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn files_logs_and_pictures_stay_bounded() {
    let dir = stand_in("bounds");
    let fresh = || VideoHost::with_module(Some(dir.join("m.wasm")));
    let opened = |host: &mut VideoHost, mode: &str| {
        let name = format!("{mode}.avi");
        let mut v = host.open(dir.join(&name).into(), &name, &format());
        let events = events_until(&mut v, |e| {
            matches!(e, VideoEvent::Opened(_) | VideoEvent::Error(_))
        });
        events.into_iter().find_map(|e| match e {
            VideoEvent::Opened(info) => Some(info),
            _ => None,
        })
    };
    // Opens past the cap are refused (and the files it leaked keep the
    // next open from working, until a fresh instance).
    let mut host = fresh();
    let info = opened(&mut host, "F").unwrap();
    assert!(info.width as usize <= MAX_OPEN_FILES, "{}", info.width);
    assert!(opened(&mut host, "N").is_none());
    // Nothing outside `/v`: every attempt refused (the first error kept).
    let mut host = fresh();
    let info = opened(&mut host, "P").unwrap();
    assert_eq!(info.width, 44, "ENOENT");
    // 64 MiB of messages without a line end: read, cut, done.
    assert!(opened(&mut host, "S").is_some());
    // A probe of a picture larger than the memory: an answer, no panic.
    let id = host.probe(&dir.join("B.avi").into(), "B.avi");
    let start = Instant::now();
    let answer = loop {
        if let Some(a) = host.probe_result(id) {
            break a;
        }
        assert!(start.elapsed().as_secs() < 20);
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert!(answer.is_err(), "{answer:?}");
    plays(&mut host, &dir);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_cache_is_checked_before_it_is_run() {
    let dir = stand_in("cache");
    let cache = dir.join("cache");
    let module = dir.join("m.wasm");
    let mut config = Config::new();
    config.epoch_interruption(true);
    let engine = Engine::new(&config).unwrap();
    // Another module's file goes once a new one is written; a temporary
    // file goes only when an hour old (another process may be writing
    // it), so this fresh one stays.
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("ddivideo-0-0-0.cwasm"), b"old").unwrap();
    std::fs::write(cache.join("ddivideo-x.cwasm.tmp-1-0"), b"partial").unwrap();
    let (_, cached) = load_module_in(&engine, &module, Some(cache.clone())).unwrap();
    assert!(!cached);
    let files: Vec<String> = std::fs::read_dir(&cache)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(!files.contains(&"ddivideo-0-0-0.cwasm".to_string()));
    let (_, cached) = load_module_in(&engine, &module, Some(cache.clone())).unwrap();
    assert!(cached);
    // Damaged in the middle, or cut short: compiled again, never run.
    let file = cache.join(cache_name(&engine, &std::fs::read(&module).unwrap()));
    let whole = std::fs::read(&file).unwrap();
    let mut bad = whole.clone();
    let mid = bad.len() / 2;
    bad[mid] ^= 0x5a;
    for broken in [bad, whole[..whole.len() - 7].to_vec()] {
        assert!(unseal(&broken).is_none());
        std::fs::write(&file, broken).unwrap();
        let (m, cached) = load_module_in(&engine, &module, Some(cache.clone())).unwrap();
        assert!(!cached);
        assert!(m.get_export("ddi_next").is_some());
        assert_eq!(std::fs::read(&file).unwrap(), whole);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_module_is_found_from_a_workspace_build() {
    let root = std::env::temp_dir().join(format!("ddi-video-find-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let module = root.join("ws/target/video/ddivideo.wasm");
    std::fs::create_dir_all(module.parent().unwrap()).unwrap();
    std::fs::write(&module, b"\0asm").unwrap();
    // Any profile's executable, a test's too.
    for exe in [
        "ws/target/desktop/ditoditoinfinito",
        "ws/target/debug/deps/video-0123",
        "ws/target/x86_64-pc-windows-msvc/release/ditoditoinfinito.exe",
    ] {
        assert_eq!(module_near(&root.join(exe)), Some(module.clone()), "{exe}");
    }
    // A package: beside the executable, first.
    let packaged = root.join("app/video/ddivideo.wasm");
    std::fs::create_dir_all(packaged.parent().unwrap()).unwrap();
    std::fs::write(&packaged, b"\0asm").unwrap();
    assert_eq!(
        module_near(&root.join("app/ditoditoinfinito")),
        Some(packaged)
    );
    // Elsewhere: none.
    assert_eq!(module_near(&root.join("bin/ditoditoinfinito")), None);
    std::fs::remove_dir_all(&root).unwrap();
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
