//! A song without a background gets a generated one, drawn from its
//! music (`docs/plans/phase-8-gaps.md`, step 5): each look shows, the
//! field filter darkens it behind the lanes as it darkens a picture, the
//! same song second draws the same picture, brightness 0 and Off show
//! the plain ground, over a song's picture the visualizer leaves the
//! picture as it would show without it, and the kaleidoscope folds the
//! song's jacket where it has one. Song folders made here, played
//! autoplayed on a fake clock with no sound device and drawn offscreen
//! through the play's view. Skipped without a GPU adapter (CI).
//! `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures.

use std::path::Path;

use ddi_desktop::gpu;
use ddi_ui::state::settings::{BackgroundFrame, GeneratedBackground, Settings};

mod common;
use common::offscreen_play::{H, OffscreenPlay, W, load, pixel};

/// The screen's ground (`ddi_actor::background::GROUND`), encoded.
const GROUND: [u8; 3] = [5, 2, 13];

/// 120 BPM, a step each beat for 7 measures (14 s).
const NOTES: &str = "#NOTES:dance-single:x:Easy:1:0,0,0,0,0:\
     1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
     1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
     1000\n0100\n0010\n0001\n;";

/// 16 s of music at 120 BPM: a low kick each beat, a burst of noise each
/// half beat and a quiet tone under them, 16-bit stereo.
fn music() -> Vec<u8> {
    let rate = 44_100u32;
    let n = rate as usize * 16;
    let mut noise = 0x1234_5678u32;
    let mut mid = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / f64::from(rate);
        let beat = (t * 2.0).fract() / 2.0;
        let half = (t * 4.0).fract() / 4.0;
        let kick = (std::f64::consts::TAU * 55.0 * beat).sin() * (-beat * 18.0).exp() * 0.8;
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        let hat = (f64::from(noise) / f64::from(u32::MAX) * 2.0 - 1.0) * (-half * 60.0).exp() * 0.3;
        let tone = (std::f64::consts::TAU * 440.0 * t).sin() * 0.05;
        mid.push((kick + hat + tone) as f32);
    }
    ddi_library::wav::write(&[&mid, &mid], rate)
}

/// A song folder under `dir` with `tags`, a light picture `light.png`
/// and a red one, `red.png`.
fn song(dir: &Path, tags: &str, music: &[u8]) -> std::io::Result<()> {
    let song = dir.join("Pack/Song");
    std::fs::create_dir_all(&song)?;
    std::fs::write(
        song.join("song.sm"),
        format!("#TITLE:Made;#ARTIST:Test;#MUSIC:music.wav;#OFFSET:0;#BPMS:0=120;{tags}{NOTES}"),
    )?;
    image::RgbaImage::from_pixel(16, 16, image::Rgba([200, 180, 160, 255]))
        .save(song.join("light.png"))
        .map_err(std::io::Error::other)?;
    image::RgbaImage::from_fn(32, 32, |x, y| {
        image::Rgba([220, 30 + ((x * 7 + y * 3) % 40) as u8, 40, 255])
    })
    .save(song.join("red.png"))
    .map_err(std::io::Error::other)?;
    std::fs::write(song.join("music.wav"), music)
}

/// The mean of each channel left of the field, every 8th pixel.
fn mean(pixels: &[u8]) -> [f64; 3] {
    let mut sum = [0.0; 3];
    let mut n = 0.0;
    for y in (0..H - 40).step_by(8) {
        for x in (4..232).step_by(8) {
            let p = pixel(pixels, (x, y));
            for (s, v) in sum.iter_mut().zip(p) {
                *s += f64::from(v);
            }
            n += 1.0;
        }
    }
    sum.map(|s| s / n)
}

fn keep(pixels: &[u8], name: &str) {
    let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") else {
        return;
    };
    let mut out = format!("P6\n{W} {H}\n255\n").into_bytes();
    for px in pixels.chunks(4) {
        out.extend_from_slice(&px[..3]);
    }
    let _ = std::fs::write(Path::new(&dir).join(format!("generated-{name}.ppm")), out);
}

/// Inside the filter's band beside the lanes (a 640 × 360 target: the
/// lanes from 248 to 392, the band from 240.6 to 395.4), under the
/// receptors.
const INSIDE: [(u32, u32); 2] = [(243, 300), (394, 200)];

/// Frames at `times` of the song under `root` played with `settings`:
/// each frame's pixels, and what the view said of its generated
/// background after.
fn play(
    adapter: &wgpu::Adapter,
    root: &Path,
    name: &str,
    settings: &Settings,
    times: &[f64],
) -> (Vec<Vec<u8>>, Option<ddi_shell::generative::Debug>) {
    let loaded = load(root);
    let mut p = OffscreenPlay::new(adapter, loaded.clone(), settings);
    p.view.set_generated(&loaded, settings, false);
    p.view.wait_for_generated();
    let mut out = Vec::new();
    p.run(
        times,
        |_, _| {},
        |t, pixels| {
            keep(pixels, &format!("{name}-{t:.0}"));
            out.push(pixels.to_vec());
        },
    );
    assert_eq!(out.len(), times.len(), "{name}");
    (out, p.view.generated())
}

/// Pixels left of the field, every 8th: the share that is not the
/// ground, and how many colors there are.
fn beside(pixels: &[u8], rows: std::ops::Range<u32>) -> (f64, usize) {
    let mut seen = std::collections::BTreeSet::new();
    let (mut lit, mut all) = (0, 0);
    for y in rows.step_by(8) {
        for x in (4..232).step_by(8) {
            let p = pixel(pixels, (x, y));
            lit += usize::from(p != GROUND);
            all += 1;
            seen.insert(p);
        }
    }
    (lit as f64 / all as f64, seen.len())
}

fn settings(look: GeneratedBackground) -> Settings {
    Settings {
        generated_background: look,
        background_frame: BackgroundFrame::Screen,
        ..Settings::default()
    }
}

#[test]
fn a_song_without_a_background_gets_one_drawn_from_its_music() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("ddi-generated-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let music = music();
    let bare = dir.join("bare");
    song(&bare, "", &music).expect("the song folder");
    let pictured = dir.join("pictured");
    song(&pictured, "#BACKGROUND:light.png;", &music).expect("the song folder");
    let times = [3.0, 6.1];

    // Each look shows, its music analyzed: the field and the
    // kaleidoscope fill the screen, the rain and the bars draw some.
    use GeneratedBackground::*;
    for (look, fills) in [
        (Field, 0.9),
        (Kaleidoscope, 0.9),
        (Rain, 0.005),
        (Bars, 0.05),
    ] {
        let (frames, debug) = play(
            &adapter,
            &bare,
            &format!("{look:?}"),
            &settings(look),
            &times,
        );
        let debug = debug.expect("a generated background");
        assert!(
            debug.analyzed.is_some_and(|n| n > 700),
            "{look:?}: {debug:?}"
        );
        assert!(
            debug.drawn > 0 && debug.size == [W, H],
            "{look:?}: {debug:?}"
        );
        for (t, f) in times.iter().zip(&frames) {
            // The bars rise from the bottom.
            let rows = if look == Bars { H / 2..H } else { 0..H };
            let (share, colors) = beside(f, rows);
            assert!(
                share >= fills && colors > 3,
                "{look:?} at {t}: {share} {colors}"
            );
        }
        assert_ne!(frames[0], frames[1], "{look:?} moves");
    }

    // The same second draws the same picture; the filter darkens it
    // behind the lanes.
    let (a, _) = play(&adapter, &bare, "field-a", &settings(Field), &times[..1]);
    let (b, _) = play(&adapter, &bare, "field-b", &settings(Field), &times[..1]);
    assert_eq!(a, b, "the same picture again");
    let unfiltered = Settings {
        field_filter: 0.0,
        ..settings(Field)
    };
    let (c, _) = play(
        &adapter,
        &bare,
        "field-unfiltered",
        &unfiltered,
        &times[..1],
    );
    for at in INSIDE {
        let (dark, light) = (pixel(&a[0], at), pixel(&c[0], at));
        assert!(
            dark.iter().zip(light).all(|(d, l)| *d <= l) && dark != light,
            "{at:?}: {dark:?} {light:?}"
        );
    }

    // Off, and backgrounds at brightness 0: the plain ground, none made.
    let dark = Settings {
        bg_brightness: 0.0,
        ..settings(Field)
    };
    for (name, s) in [("off", settings(Off)), ("dark", dark)] {
        let (frames, debug) = play(&adapter, &bare, name, &s, &times[..1]);
        // Above the HUD's title watermark, at the bottom left.
        assert_eq!(beside(&frames[0], 0..H - 40).0, 0.0, "{name}");
        assert!(debug.is_none(), "{name}: {debug:?}");
    }

    // A song with a picture shows it; over it, the bars rise on it and
    // the rest of the picture shows as it would without them.
    let over = Settings {
        generated_over_pictures: true,
        ..settings(Bars)
    };
    let (plain, debug) = play(
        &adapter,
        &pictured,
        "pictured",
        &settings(Bars),
        &times[..1],
    );
    assert!(debug.is_some_and(|d| d.drawn == 0 && d.analyzed.is_none()));
    let (with, debug) = play(&adapter, &pictured, "pictured-over", &over, &times[..1]);
    assert!(debug.is_some_and(|d| d.drawn > 0 && d.analyzed.is_some()));
    let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2);
    let top = pixel(&plain[0], (20, 20));
    assert!(top != GROUND && near(pixel(&with[0], (20, 20)), top));
    let changed = (4..232)
        .step_by(4)
        .filter(|x| pixel(&with[0], (*x, H - 4)) != pixel(&plain[0], (*x, H - 4)))
        .count();
    assert!(changed > 10, "{changed}");

    // The kaleidoscope folds the song's jacket (else its banner) where
    // it has no background: red, where its glass is the palette's blue
    // gray.
    let jacketed = dir.join("jacketed");
    song(&jacketed, "", &music).expect("the song folder");
    // Found by its name, as StepMania finds one.
    let folder = jacketed.join("Pack/Song");
    std::fs::rename(folder.join("red.png"), folder.join("jacket.png")).expect("renamed");
    // Without pictures (a picture in the folder may be taken for its
    // banner, which the kaleidoscope folds too).
    let imageless = dir.join("imageless");
    song(&imageless, "", &music).expect("the song folder");
    for name in ["light.png", "red.png"] {
        std::fs::remove_file(imageless.join("Pack/Song").join(name)).expect("removed");
    }
    let (glass, _) = play(
        &adapter,
        &imageless,
        "glass",
        &settings(Kaleidoscope),
        &times[..1],
    );
    let (folded, debug) = play(
        &adapter,
        &jacketed,
        "jacket",
        &settings(Kaleidoscope),
        &times[..1],
    );
    assert!(debug.is_some_and(|d| d.drawn > 0));
    let [gr, _, gb] = mean(&glass[0]);
    let [fr, _, fb] = mean(&folded[0]);
    assert!(gb > gr, "glass {gr} {gb}");
    assert!(fr > fb * 1.5 && fr > gr, "jacket {fr} {fb}, glass {gr}");
    // The other looks never draw it.
    let (field, _) = play(
        &adapter,
        &jacketed,
        "jacket-field",
        &settings(Field),
        &times[..1],
    );
    let (bare_field, _) = play(
        &adapter,
        &imageless,
        "bare-field",
        &settings(Field),
        &times[..1],
    );
    assert_eq!(field, bare_field);
    let _ = std::fs::remove_dir_all(&dir);
}
