//! Both background layers on desktop: a song folder made here (solid
//! color images, one half transparent, and with feature `video` a movie
//! on the second layer), played autoplayed on a fake clock with no sound
//! device, drawn offscreen through the play's view, a pixel left of the
//! field read back at chosen song times. The second layer (`#BGCHANGES2`)
//! draws over the first as StepMania draws it
//! (`ddi_library::backgrounds::second_layer`). Skipped without a GPU
//! adapter (and the movie without the `ffmpeg` command or the helper).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::gpu;
use ddi_desktop::library::files::{DirReader, walk};
use ddi_desktop::play::load::{Files, Loaded};
use ddi_desktop::play::view::{Formats, Rgba, View};
use ddi_desktop::play::{Feed, Play, Request};
use ddi_platform::HostTime;
use ddi_platform::decode_all::Range;
use ddi_ui::state::settings::{BackgroundFrame, Settings, VideoMode};

const W: u32 = 640;
const H: u32 = 360;

/// 120 BPM: beat `b` at `b / 2` s. The first layer: the song background
/// (blue), red from 2 s, the song background again from 7 s. The second:
/// nothing until 4 s, then a green image at half opacity over the red, a
/// crossfade (field 4) to opaque white at 6 s, a missing file at 8 s that
/// changes nothing, and (`movie`) a movie from 9 s.
fn simfile(movie: bool) -> String {
    let movie = if movie { ",18=clip.avi=1=0=0=1" } else { "" };
    format!(
        "#TITLE:Layers;#ARTIST:Test;#MUSIC:music.wav;#OFFSET:0;#BPMS:0=120;\
         #BACKGROUND:bg.png;\
         #BGCHANGES:4=red.png=1=0=0=0,14=bg.png=1=0=0=0,99999=-nosongbg-;\
         #BGCHANGES2:8=half.png=1=0=0=0,12=white.png=1=1=0=0,16=missing.png=1=0=0=0{movie};\
         #NOTES:dance-single:x:Easy:1:0,0,0,0,0:\
         1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
         1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
         1000\n0100\n0010\n0001\n;"
    )
}

fn ffmpeg(args: &[&str]) -> bool {
    std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

/// The song folder under `dir`: its images, its music (silence, written
/// here), and with `movie` a yellow clip (needs `ffmpeg`).
fn song(dir: &Path, movie: bool) -> Option<()> {
    let song = dir.join("Pack/Layers");
    std::fs::create_dir_all(&song).ok()?;
    std::fs::write(song.join("song.sm"), simfile(movie)).ok()?;
    for (name, rgba) in [
        ("bg.png", [30, 30, 200, 255]),
        ("red.png", [200, 30, 30, 255]),
        ("half.png", [30, 200, 30, 128]),
        ("white.png", [230, 230, 230, 255]),
    ] {
        image::RgbaImage::from_pixel(16, 16, image::Rgba(rgba))
            .save(song.join(name))
            .ok()?;
    }
    // 16 s of 16-bit stereo silence.
    let rate = 44_100u32;
    let data = vec![0u8; (rate * 4 * 16) as usize];
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    std::fs::write(song.join("music.wav"), wav).ok()?;
    if movie {
        let clip = song.join("clip.avi");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "color=c=0xC0C000:size=320x240:rate=30",
            "-t",
            "5",
            "-c:v",
            "mpeg4",
            "-q:v",
            "1",
            clip.to_str()?,
        ])
        .then_some(())?;
    }
    Some(())
}

/// Plays the song under `root` for `seconds`, a frame each 1/60 s, and
/// reads the pixel at (20, 180) (left of the field) at the first frame at
/// or after each of `times` (song seconds): `(time drawn, [r, g, b])`.
fn play(adapter: &wgpu::Adapter, root: &Path, times: &[f64]) -> Vec<(f64, [u8; 3])> {
    let paths: Vec<_> = walk(root).listed.into_iter().map(|l| l.path).collect();
    let reader = DirReader {
        root: root.to_path_buf(),
    };
    let scan =
        ddi_library::scan::scan_root(&reader, &paths, "Mine", &[], &ddi_library::legacy::none);
    let s = &scan.songs[0];
    let files = Files::Dir {
        root: root.to_path_buf(),
        dir: s.dir.clone(),
    };
    let text = ddi_library::pack::decode_text(&files.read(&s.entry.chart).unwrap());
    let parsed = ddi_chart::formats::sm::parse_simfile(&text, "sm").unwrap();
    let music = ddi_desktop::audio::decoder::sound(
        files.read(&s.entry.music).unwrap(),
        Some("wav"),
        Range::default(),
        48_000,
    )
    .unwrap();
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
        bg_brightness: 1.0,
        field_filter: 0.0,
        background_frame: BackgroundFrame::Screen,
        video: VideoMode::On,
        ..Settings::default()
    };
    let mut view = View::new(
        &device,
        &queue,
        Formats::of(wgpu::TextureFormat::Rgba8Unorm, true),
    );
    view.set_song(&loaded, &settings);
    // Read the pictures here, as the host's threads read them.
    for (what, source, side) in View::images_to_read(&loaded, &settings) {
        let (size, data) = ddi_desktop::images::load(&source, side).expect("an image");
        view.add_background(what, Rgba { size, data });
    }
    view.start();
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let mut play = Play::start(
        Request::song(&s.entry.id, 0, true),
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
            width: W,
            height: H,
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
    let render = ddi_shell::render_options(&settings);
    #[cfg(feature = "video")]
    {
        let start = std::time::Instant::now();
        while view.movies.helper_status().is_some()
            && !matches!(
                view.movies.helper_status(),
                Some(
                    ddi_desktop::video::HelperStatus::Ready { .. }
                        | ddi_desktop::video::HelperStatus::Failed(_)
                )
            )
            && start.elapsed().as_secs() < 60
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    let mut out = Vec::new();
    let mut next = 0;
    for _ in 0..60 * 30 {
        if next == times.len() {
            break;
        }
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        let now = HostTime(now as f64 / 1e9);
        play.tick(now, None);
        let t = play
            .session
            .player
            .frame(play.session.predicted_present(now))
            .song_time;
        let mut encoder = device.create_command_encoder(&Default::default());
        view.draw(
            &device,
            &queue,
            &mut encoder,
            &srgb,
            Some(&plain),
            [W, H],
            &play,
            render,
            now,
        );
        if t >= times[next] {
            next += 1;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(W * 4 * H),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(W * 4),
                        rows_per_image: Some(H),
                    },
                },
                texture.size(),
            );
            queue.submit(Some(encoder.finish()));
            let slice = buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let pixels = slice.get_mapped_range().expect("mapped").to_vec();
            let i = ((180 * W + 20) * 4) as usize;
            out.push((t, [pixels[i], pixels[i + 1], pixels[i + 2]]));
        } else {
            queue.submit(Some(encoder.finish()));
        }
        // The decoder thread's turn.
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    out
}

fn near(got: [u8; 3], want: [u8; 3], tolerance: i32) -> bool {
    got.iter()
        .zip(want)
        .all(|(&g, w)| (i32::from(g) - i32::from(w)).abs() <= tolerance)
}

/// `a` blended over `b` at `alpha` in linear light (the sRGB view), as
/// encoded values.
fn over(a: [u8; 3], alpha: f64, b: [u8; 3]) -> [u8; 3] {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let enc = |l: f64| {
        let c = if l <= 0.003_130_8 {
            l * 12.92
        } else {
            1.055 * l.powf(1.0 / 2.4) - 0.055
        };
        (c * 255.0).round() as u8
    };
    std::array::from_fn(|i| enc(lin(a[i]) * alpha + lin(b[i]) * (1.0 - alpha)))
}

const BLUE: [u8; 3] = [30, 30, 200];
const RED: [u8; 3] = [200, 30, 30];
const GREEN: [u8; 3] = [30, 200, 30];
const WHITE: [u8; 3] = [230, 230, 230];
const HALF: f64 = 128.0 / 255.0;

/// What the pixel shows at song second `t`, while it holds: `None` within
/// a frame of a change.
fn expected(t: f64) -> Option<[u8; 3]> {
    let green_on_red = over(GREEN, HALF, RED);
    Some(match t {
        t if t < 1.9 => BLUE,
        t if (2.1..3.9).contains(&t) => RED,
        t if (4.1..5.9).contains(&t) => green_on_red,
        // The crossfade: white fully drawn, the green over it fading out.
        t if (6.45..6.55).contains(&t) => over(GREEN, HALF * 0.5, WHITE),
        // White covers the first layer's return to blue at 7 s, and the
        // missing file at 8 s changes nothing.
        t if (7.1..8.9).contains(&t) => WHITE,
        _ => return None,
    })
}

/// One test, so the environment is set before any thread runs (the
/// module's cache folder for the movie part).
#[test]
fn the_second_layer_draws_over_the_first() {
    let dir = std::env::temp_dir().join(format!("ddi-play-layers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let images = dir.join("images");
    song(&images, false).expect("the song folder");
    let times = [1.0, 3.0, 5.0, 6.5, 7.5, 8.5];
    let samples = play(&adapter, &images, &times);
    assert_eq!(samples.len(), times.len(), "{samples:?}");
    for (t, got) in samples {
        let want = expected(t).unwrap_or_else(|| panic!("no expectation at {t}"));
        assert!(near(got, want, 3), "at {t:.3} s: {got:?}, want {want:?}");
    }
    #[cfg(feature = "video")]
    a_movie_plays_on_the_second_layer(&adapter, &dir);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The helper is found beside the test binary's profile folder
/// (`crates/desktop/build.rs`); without it, or without `ffmpeg`, this part is skipped.
#[cfg(feature = "video")]
fn a_movie_plays_on_the_second_layer(adapter: &wgpu::Adapter, dir: &Path) {
    if ddi_desktop::video::helper_path().is_none() {
        eprintln!("no video helper: movie skipped");
        return;
    }
    let movie = dir.join("movie");
    if song(&movie, true).is_none() {
        eprintln!("no ffmpeg: movie skipped");
        return;
    }
    let samples = play(adapter, &movie, &[7.5, 10.0, 11.0]);
    assert_eq!(samples.len(), 3, "{samples:?}");
    assert!(near(samples[0].1, WHITE, 3), "{samples:?}");
    // The movie's yellow (limited range, BT.601) over the white.
    for (t, got) in &samples[1..] {
        assert!(near(*got, [192, 192, 0], 12), "at {t:.3} s: {got:?}");
    }
}
