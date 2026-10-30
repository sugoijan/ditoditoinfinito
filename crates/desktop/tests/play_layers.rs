//! Both background layers on desktop: a song folder made here (solid
//! color images, one half transparent, and with feature `video` a movie
//! on the second layer), played autoplayed on a fake clock with no sound
//! device, drawn offscreen through the play's view, a pixel left of the
//! field read back at chosen song times. The second layer (`#BGCHANGES2`)
//! draws over the first as StepMania draws it
//! (`ddi_library::backgrounds::second_layer`). Skipped without a GPU
//! adapter (and the movie without the `ffmpeg` command or the helper).

use std::path::Path;

use ddi_desktop::gpu;
use ddi_ui::state::settings::{BackgroundFrame, Settings, VideoMode};

mod common;
use common::offscreen_play::{OffscreenPlay, load, pixel};

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
    let settings = Settings {
        bg_brightness: 1.0,
        field_filter: 0.0,
        background_frame: BackgroundFrame::Screen,
        video: VideoMode::On,
        ..Settings::default()
    };
    let mut p = OffscreenPlay::new(adapter, load(root), &settings);
    #[cfg(feature = "video")]
    {
        let start = std::time::Instant::now();
        while p.view.movies.helper_status().is_some()
            && !matches!(
                p.view.movies.helper_status(),
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
    p.run(
        times,
        // The decoder thread's turn.
        |_, _| std::thread::sleep(std::time::Duration::from_millis(2)),
        |t, pixels| out.push((t, pixel(pixels, (20, 180)))),
    );
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
