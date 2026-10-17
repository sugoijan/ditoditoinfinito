//! A play's background movie on desktop (feature `video`): a song folder
//! made here (a clip from FFmpeg's test pattern, its `#BGCHANGES` at the
//! first beat), listed by the library's scan, played on a fake clock with
//! no sound device; MPEG-4 opens on the module and H.264 uses the OS path
//! where its capability gate and hardware session permit it;
//! its frames reach the renderer and show behind the field. Skipped
//! without a GPU adapter, the `ffmpeg` command or the module (`just
//! video`).
#![cfg(feature = "video")]

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::gpu;
use ddi_desktop::library::files::{DirReader, walk};
use ddi_desktop::play::load::{Files, Loaded};
use ddi_desktop::play::view::{Formats, View, render_options};
use ddi_desktop::play::{Feed, Play, Request};
use ddi_platform::HostTime;
use ddi_platform::decode_all::Range;
use ddi_ui::state::settings::Settings;

fn ffmpeg(args: &[&str]) -> bool {
    std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

const SM: &str = "#TITLE:Movie;#ARTIST:Test;#MUSIC:music.wav;#OFFSET:0;#BPMS:0=120;\
#BGCHANGES:0=clip.avi=1.000=0=0=1;\
#NOTES:dance-single:x:Easy:1:0,0,0,0,0:\
1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n;";

fn song(dir: &Path, codec: &str, extension: &str) -> Option<()> {
    let song = dir.join("Pack/Movie");
    std::fs::create_dir_all(&song).ok()?;
    std::fs::write(
        song.join("song.sm"),
        SM.replace("clip.avi", &format!("clip.{extension}")),
    )
    .ok()?;
    let clip = song.join(format!("clip.{extension}"));
    let music = song.join("music.wav");
    (ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x240:rate=30",
        "-t",
        "6",
        "-c:v",
        codec,
        clip.to_str()?,
    ]) && ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "anullsrc=r=44100:cl=stereo",
        "-t",
        "12",
        music.to_str()?,
    ]))
    .then_some(())
}

#[test]
fn a_movie_plays_behind_the_field() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let dev_module = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/video/ddivideo.wasm");
    if ddi_desktop::video::module_path().is_none() && !dev_module.is_file() {
        eprintln!("no video module (just video): skipped");
        return;
    }
    if ddi_desktop::video::module_path().is_none() {
        // SAFETY: set before any thread of this test binary reads it (one
        // test here).
        unsafe { std::env::set_var("DDI_VIDEO_MODULE", &dev_module) };
    }
    for (codec, extension) in [
        ("mpeg4", "avi"),
        ("libx264", "avi"),
        ("libx264", "mp4"),
        ("libx264", "mkv"),
    ] {
        let dir = std::env::temp_dir().join(format!(
            "ddi-play-movie-{}-{codec}-{extension}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        if song(&dir, codec, extension).is_none() {
            eprintln!("no ffmpeg with {codec}: skipped");
            continue;
        }
        // SAFETY: this binary's only non-ignored test; before its host starts.
        unsafe { std::env::set_var("DDI_VIDEO_CACHE", dir.join("cache")) };
        let (stats, shown, trouble, _) = play_movies(&adapter, &dir, 8.0);
        assert!(shown, "{codec}: no movie frame shown: {stats:?}");
        assert!(stats.shown > 30, "{codec}: {stats:?}");
        assert!(trouble.is_none(), "{codec}: {trouble:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The first song with a movie under `root` (scanned as the library
/// scans), played autoplayed on a fake clock for `seconds` of song time,
/// a frame each 1/60 s, the decoder thread given real time between
/// frames: the movies' counters, whether a frame showed, what kept them
/// from playing, the last frame's pixels (640×360 RGBA).
fn play_movies(
    adapter: &wgpu::Adapter,
    root: &Path,
    seconds: f64,
) -> (
    ddi_platform::video::MovieStats,
    bool,
    Option<String>,
    Vec<u8>,
) {
    let paths: Vec<_> = walk(root).listed.into_iter().map(|l| l.path).collect();
    let reader = DirReader {
        root: root.to_path_buf(),
    };
    let scan = ddi_library::scan::scan_root(&reader, &paths, "Mine", &[], &|_| None);
    let s = scan
        .songs
        .iter()
        .find(|s| s.entry.bg_videos.iter().any(|v| v.plays()))
        .expect("a song with a movie");
    let files = Files::Dir {
        root: root.to_path_buf(),
        dir: s.dir.clone(),
    };
    let text = ddi_library::pack::decode_text(&files.read(&s.entry.chart).unwrap());
    let ext = s
        .entry
        .chart
        .as_str()
        .rsplit_once('.')
        .map_or("sm", |(_, e)| e)
        .to_ascii_lowercase();
    let parsed = ddi_chart::formats::sm::parse_simfile(&text, &ext).unwrap();
    let music_ext = s
        .entry
        .music
        .as_str()
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    let music = ddi_desktop::audio::decoder::sound(
        files.read(&s.entry.music).unwrap(),
        music_ext.as_deref(),
        Range::default(),
        48_000,
    )
    .unwrap();
    let chart = parsed
        .charts
        .iter()
        .position(|c| parsed.layout_of(c).is_some())
        .expect("a playable chart");
    let movies = ddi_desktop::play::load::movies(&files, &s.entry);
    let loaded = Rc::new(Loaded {
        entry: s.entry.clone(),
        song: parsed,
        files,
        music,
        movies,
        layers: Vec::new(),
    });
    let (device, queue) = gpu::device(adapter).unwrap();
    let settings = Settings {
        noteskin: Some("builtin".into()),
        ..Settings::default()
    };
    let mut view = View::new(
        &device,
        &queue,
        Formats::of(wgpu::TextureFormat::Rgba8Unorm, true),
    );
    view.set_song(&loaded, &settings);
    view.start();
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let mut play = Play::start(
        Request::song(&s.entry.id, chart, true),
        loaded,
        &settings,
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )
    .unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 640,
            height: 360,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());
    let render = render_options(&settings);
    // The module compiled (or read from the cache) before the song.
    let start = std::time::Instant::now();
    while !matches!(
        view.movies.module_status(),
        Some(
            ddi_desktop::video::ModuleStatus::Ready { .. }
                | ddi_desktop::video::ModuleStatus::Failed(_)
        )
    ) && start.elapsed().as_secs() < 60
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    println!("module: {:?}", view.movies.module_status());
    let frames = (seconds * 60.0) as usize;
    let mut pixels = Vec::new();
    for i in 0..frames {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        let now = HostTime(now as f64 / 1e9);
        play.tick(now, None);
        let mut encoder = device.create_command_encoder(&Default::default());
        view.draw(
            &device,
            &queue,
            &mut encoder,
            &srgb,
            Some(&plain),
            [640, 360],
            &play,
            render,
            now,
        );
        if i + 1 == frames {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 640 * 4 * 360,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(640 * 4),
                        rows_per_image: Some(360),
                    },
                },
                texture.size(),
            );
            queue.submit(Some(encoder.finish()));
            let slice = buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            pixels = slice.get_mapped_range().expect("mapped").to_vec();
        } else {
            queue.submit(Some(encoder.finish()));
        }
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
    let stats = view.movies.stats();
    println!("movie: {stats:?}");
    (
        stats,
        view.movies.shown_any(),
        view.movies.trouble(),
        pixels,
    )
}

/// A local song's movie (`DDI_MOVIE_SONGS=<folder>`, a pack or a song
/// folder; `DDI_MOVIE_SECONDS`, 20 by default): it plays, frames shown,
/// none late beyond a few; `DDI_OFFSCREEN_PPM=<dir>` keeps the last frame.
/// Third-party songs stay local.
#[test]
#[ignore = "needs DDI_MOVIE_SONGS"]
fn a_local_songs_movie_plays() {
    let Some(root) = std::env::var_os("DDI_MOVIE_SONGS").map(std::path::PathBuf::from) else {
        return;
    };
    let seconds = std::env::var("DDI_MOVIE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20.0);
    let instance = gpu::instance();
    let adapter = gpu::adapter(&instance, None).expect("a GPU");
    let dev_module = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/video/ddivideo.wasm");
    if ddi_desktop::video::module_path().is_none() {
        // SAFETY: set before any thread of this test binary reads it.
        unsafe { std::env::set_var("DDI_VIDEO_MODULE", &dev_module) };
    }
    let (stats, shown, trouble, pixels) = play_movies(&adapter, &root, seconds);
    if let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") {
        let mut out = b"P6\n640 360\n255\n".to_vec();
        for px in pixels.chunks(4) {
            out.extend_from_slice(&px[..3]);
        }
        std::fs::write(Path::new(&dir).join("play-movie.ppm"), out).unwrap();
    }
    assert!(shown, "no movie frame shown: {stats:?}");
    assert!(trouble.is_none(), "{trouble:?}");
    assert!(stats.shown as f64 > seconds * 10.0, "{stats:?}");
}
