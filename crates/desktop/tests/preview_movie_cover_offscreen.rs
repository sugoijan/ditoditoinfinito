//! Song select's preview movie on desktop (feature `video`,
//! `docs/plans/lane-covers.md`, "Preview video"; since round SLB in the
//! cover square, `docs/plans/song-navigation.md`, "The song's side"): a
//! song folder made here (a blue 3.2:1 banner and `#PREVIEWVID` naming a
//! 16:9 H.264 clip of a color turning round the hue wheel, both generated
//! by the `ffmpeg` command), listed by the library's scan, selected on
//! song select and drawn offscreen. Before the movie the square shows
//! the banner whole over its blurred copy; once the selection has settled
//! the movie's frames fade in over it (0.25 s: part-way a few frames in),
//! whole at its own proportion across the square, and keep changing;
//! another song selected stops it and shows that song's art. Skipped
//! without a GPU adapter, the `ffmpeg` command or the video helper.
//! `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures (`.png` names).
#![cfg(feature = "video")]

mod common;
use std::path::Path;
use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::songs::Songs;
use ddi_ui::UiEvent;
use ddi_ui::screens::select::PREVIEW_MOVIE;
use ddi_ui::state::place::MenuPlace;

fn ffmpeg(args: &[&str]) -> bool {
    std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

const SSC: &str = "#VERSION:0.83;#TITLE:Preview Movie;#ARTIST:Test;#MUSIC:music.wav;\
#BANNER:banner.png;#PREVIEWVID:Preview.mp4;#OFFSET:0;#BPMS:0=120;#SAMPLESTART:0;#SAMPLELENGTH:4;\
#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Easy;#METER:1;#NOTES:\n\
1000\n0100\n0010\n0001\n;";

/// The song folder, or `None` without `ffmpeg`.
fn song(dir: &Path) -> Option<()> {
    let song = dir.join("Pack/Preview Movie");
    std::fs::create_dir_all(&song).ok()?;
    std::fs::write(song.join("song.ssc"), SSC).ok()?;
    let clip = song.join("Preview.mp4");
    let music = song.join("music.wav");
    let banner = song.join("banner.png");
    (ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=0x2040ff:s=512x160",
        "-frames:v",
        "1",
        banner.to_str()?,
    ]) && ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=red:s=320x180:r=30:d=3,hue=h=t*120:s=1",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-movflags",
        "+faststart",
        clip.to_str()?,
    ]) && ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "anullsrc=r=44100:cl=stereo",
        "-t",
        "6",
        music.to_str()?,
    ]))
    .then_some(())
}

#[test]
fn the_preview_movie_plays_in_the_cover_square() {
    common::indexes();
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/preview-movie-cover-offscreen/home");
    let _ = std::fs::remove_dir_all(&home);
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", &home) };
    ddi_desktop::audio::set_headless(true);
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    if ddi_desktop::video::helper_path().is_none() {
        eprintln!("no video helper: skipped");
        return;
    }
    let dir = home.join("../songs");
    if song(&dir).is_none() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(|| {});
    // The library scanned once (its cache kept), the song's id found and
    // kept as the list's place: the next song select shows it.
    let id = {
        let songs = Songs::new(vec![dir.clone()]);
        songs.skip_offer();
        songs.start(wake.clone());
        let started = std::time::Instant::now();
        loop {
            let id = songs.library.borrow().as_ref().and_then(|l| {
                (!l.scanning)
                    .then(|| l.songs.iter().find(|s| s.entry.title == "Preview Movie"))
                    .flatten()
                    .map(|s| s.entry.id.clone())
            });
            if let Some(id) = id {
                break id;
            }
            let mut ui = ddi_ui::Ui::new(ddi_ui::screens::DESIGN, ddi_ui::TextMode::Own, true);
            songs.poll(&mut ui);
            assert!(started.elapsed().as_secs() < 30, "the scan never ended");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };
    MenuPlace {
        known: true,
        song: Some(id.clone()),
        chart: Some((id.clone(), 0)),
        ..MenuPlace::default()
    }
    .save();
    let songs = Songs::new(vec![dir.clone()]);
    songs.skip_offer();
    songs.start(wake);
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let sound = Rc::new(std::cell::RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::preview::PreviewSong::bundled,
    )));
    let (mut ui, env, _) = ddi_desktop::shell::library_ui("select", fold, sound, &songs);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    let keep = |name: &str, pixels: &[u8]| {
        common::shot(name, pixels, [1280, 720]);
    };
    // A part of the square: its picture's source and its alpha now.
    let part = |ui: &ddi_ui::Ui, name: &str| -> Option<(String, f32)> {
        let mut info = ui.actor("Info")?.clone();
        let a = ddi_ui::widgets::part(&mut info, name)?;
        let ddi_actor::Kind::Sprite(s) = &a.kind else {
            return None;
        };
        let alpha = a.current().diffuse[0][3];
        a.visible
            .then(|| ui.texture_source(s.texture))
            .flatten()
            .map(|t| (t.source.to_string(), alpha))
    };
    // The cover square's pixels: a row's mean color at a share of its
    // height (0.5 the middle), across its inner width.
    let row = |ui: &ddi_ui::Ui, pixels: &[u8], at: f32| -> [u32; 3] {
        let [x0, y0, x1, y1] = ui.bounds("Cover").expect("the cover square");
        let y = (y0 + (y1 - y0) * at) as u32;
        let (xa, xb) = (x0 as u32 + 6, x1 as u32 - 6);
        let mut sum = [0u64; 3];
        for x in xa..xb {
            let i = ((y * 1280 + x) * 4) as usize;
            for c in 0..3 {
                sum[c] += u64::from(pixels[i + c]);
            }
        }
        sum.map(|s| (s / u64::from(xb - xa)) as u32)
    };
    let bright = |c: [u32; 3]| c.iter().copied().max().unwrap_or(0);
    let frame = |ui: &mut ddi_ui::Ui, gpu: &mut Gpu| {
        songs.poll(ui);
        ui.update(1.0 / 60.0);
        gpu.movie
            .follow(songs.select_shown.get(), &env, Some(&songs.feed));
        let pixels = gpu.draw_offscreen(ui, [1280, 720], format);
        std::thread::sleep(std::time::Duration::from_millis(16));
        pixels
    };
    // From the first frame: the banner whole in the square over its blur
    // before the movie; then (the helper starting takes a while) the
    // movie's frames fading in over it, part-way a few frames in, whole
    // by 0.25 s, the banner out.
    let started = std::time::Instant::now();
    let partway = |ui: &ddi_ui::Ui| part(ui, "CoverMovie").is_some_and(|(_, a)| a > 0.05 && a < 0.95);
    let (mut seen_before, mut seen_partway) = (false, false);
    let mut pixels;
    loop {
        pixels = frame(&mut ui, &mut gpu);
        let movie = part(&ui, "CoverMovie").filter(|(_, a)| *a > 0.0);
        // (The last frame before the movie: the screen may still be
        // coming in, the helper being quick.)
        if movie.is_none()
            && let Some((art, a)) = part(&ui, "CoverArt")
        {
            assert!(art.ends_with("banner.png") && a > 0.999, "{art} {a}");
            assert!(part(&ui, "CoverBlur").is_some(), "its blurred copy under it");
            seen_before = true;
            keep("preview-cover-0-before.png", &pixels);
        }
        if partway(&ui) && !seen_partway {
            seen_partway = true;
            keep("preview-cover-1-fading.png", &pixels);
        }
        if gpu.movie.state().frames() >= 5 && part(&ui, "CoverMovie").is_some_and(|(_, a)| a >= 0.999) {
            break;
        }
        assert!(
            started.elapsed().as_secs() < 30,
            "no frame came: {:?}",
            gpu.movie.state()
        );
    }
    assert!(seen_before, "the banner before the movie");
    assert_eq!(env.state.borrow().song_id, id);
    assert_eq!(gpu.movie.state().showing(), Some(id.as_str()));
    assert!(gpu.movie.busy());
    assert!(seen_partway, "the movie faded in");
    let movie = part(&ui, "CoverMovie").expect("the movie in the square");
    assert_eq!(movie.0, PREVIEW_MOVIE);
    assert!(movie.1 > 0.999, "{movie:?}");
    assert!(part(&ui, "CoverArt").is_none_or(|(_, a)| a < 0.001), "the banner faded out");
    for _ in 0..3 {
        pixels = frame(&mut ui, &mut gpu);
    }
    keep("preview-cover-2-playing.png", &pixels);
    // Whole at its own proportion (16:9 across the square: the middle
    // row the movie's color edge to edge, the top band the banner's
    // blurred copy: letterboxed over the blur, not cropped to the
    // square), and changing.
    let middle = row(&ui, &pixels, 0.5);
    let top = row(&ui, &pixels, 0.06);
    let diff = |a: [u32; 3], b: [u32; 3]| -> u32 { a.iter().zip(b).map(|(x, y)| x.abs_diff(y)).sum() };
    assert!(bright(middle) > 100, "the movie across the middle: {middle:?}");
    // (The blue banner's blur, dimmed: blue well over red and green.)
    assert!(top[2] > top[0] + 50 && top[2] > top[1] + 50, "the blur above it: {top:?}");
    assert!(diff(top, middle) > 100, "letterboxed, not cropped: {top:?} {middle:?}");
    let first = middle;
    for _ in 0..50 {
        pixels = frame(&mut ui, &mut gpu);
    }
    keep("preview-cover-3-later.png", &pixels);
    let later = row(&ui, &pixels, 0.5);
    let moved: u32 = first.iter().zip(later).map(|(a, b)| a.abs_diff(b)).sum();
    assert!(moved > 60, "the picture changed: {first:?} → {later:?}");
    assert!(gpu.movie.state().frames() > 20, "{:?}", gpu.movie.state());
    // Another song: the movie stops, its art shows at once.
    for key in ["ArrowUp", "ArrowDown", "ArrowDown"] {
        if env.state.borrow().song_id != id {
            break;
        }
        ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down(key, key)));
        ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up(key, key)));
        for _ in 0..30 {
            pixels = frame(&mut ui, &mut gpu);
        }
    }
    keep("preview-cover-4-after.png", &pixels);
    assert_ne!(env.state.borrow().song_id, id);
    assert_eq!(gpu.movie.state().showing(), None);
    assert!(!gpu.movie.busy());
    assert!(part(&ui, "CoverMovie").is_none_or(|(s, a)| s != PREVIEW_MOVIE || a < 0.001));
    assert!(part(&ui, "CoverArt").is_none_or(|(_, a)| a > 0.999), "the next song's art whole");
    let _ = std::fs::remove_dir_all(&dir);
}
