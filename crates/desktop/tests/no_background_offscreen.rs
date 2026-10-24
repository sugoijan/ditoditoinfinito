//! A song without a background plays on one ground: the field filter
//! darkens a background behind the lanes, and over the plain ground
//! alone (no `#BACKGROUND`, a picture that is not there, the time before
//! a song's first change, backgrounds' brightness at 0) it draws nothing,
//! so the lanes and the screen around them are the same color, whatever
//! the filter and the brightness (`docs/plans/overlays.md`, "Field
//! filter"). A song with a background keeps its band. Song folders made
//! here, played autoplayed on a fake clock with no sound device and
//! drawn offscreen through the play's view. Skipped without a GPU
//! adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures.

use std::path::Path;

use ddi_desktop::gpu;
use ddi_ui::state::settings::{BackgroundFrame, Settings};

mod common;
use common::offscreen_play::{H, OffscreenPlay, W, load, pixel};

/// The screen's ground (`ddi_actor::background::GROUND`), encoded.
const GROUND: [u8; 3] = [5, 2, 13];

/// 120 BPM, a step each beat for 7 measures (14 s).
const NOTES: &str = "#NOTES:dance-single:x:Easy:1:0,0,0,0,0:\
     1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
     1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
     1000\n0100\n0010\n0001\n;";

/// A song folder under `dir` with `tags` (its background's), a light
/// image `light.png` and silent music.
fn song(dir: &Path, tags: &str) -> std::io::Result<()> {
    let song = dir.join("Pack/Song");
    std::fs::create_dir_all(&song)?;
    std::fs::write(
        song.join("song.sm"),
        format!("#TITLE:Ground;#ARTIST:Test;#MUSIC:music.wav;#OFFSET:0;#BPMS:0=120;{tags}{NOTES}"),
    )?;
    image::RgbaImage::from_pixel(16, 16, image::Rgba([200, 180, 160, 255]))
        .save(song.join("light.png"))
        .map_err(std::io::Error::other)?;
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
    std::fs::write(song.join("music.wav"), wav)
}

/// Beside the field, and inside the filter's band beside the lanes (a
/// 640 × 360 target: 36-pixel arrows, the lanes from 248 to 392, the
/// band from 240.6 to 395.4), under the receptors.
const OUTSIDE: [(u32, u32); 2] = [(20, 300), (600, 200)];
const INSIDE: [(u32, u32); 2] = [(243, 300), (394, 200)];

fn keep(pixels: &[u8], name: &str) {
    let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") else {
        return;
    };
    let mut out = format!("P6\n{W} {H}\n255\n").into_bytes();
    for px in pixels.chunks(4) {
        out.extend_from_slice(&px[..3]);
    }
    let _ = std::fs::write(Path::new(&dir).join(format!("ground-{name}.ppm")), out);
}

/// A frame's time and its pixels beside the field and inside its band.
type Sample = (f64, [[u8; 3]; 2], [[u8; 3]; 2]);

/// The song under `root` played with backgrounds at `brightness` and
/// the field filter at `filter`: at each of `times`, the pixels beside
/// the field and inside its band.
fn play(
    adapter: &wgpu::Adapter,
    root: &Path,
    name: &str,
    (brightness, filter): (f32, f32),
    times: &[f64],
) -> Vec<Sample> {
    let settings = Settings {
        noteskin: Some("builtin".into()),
        bg_brightness: brightness,
        field_filter: filter,
        background_frame: BackgroundFrame::Screen,
        ..Settings::default()
    };
    let mut p = OffscreenPlay::new(adapter, load(root), &settings);
    let mut out = Vec::new();
    p.run(
        times,
        |_, _| {},
        |t, pixels| {
            keep(pixels, &format!("{name}-{brightness}-{filter}-{t:.0}"));
            out.push((
                t,
                OUTSIDE.map(|at| pixel(pixels, at)),
                INSIDE.map(|at| pixel(pixels, at)),
            ));
        },
    );
    assert_eq!(out.len(), times.len(), "{name}: {out:?}");
    out
}

#[test]
fn a_song_without_a_background_plays_on_one_ground() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("ddi-no-background-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let folder = |name: &str, tags: &str| {
        let root = dir.join(name);
        song(&root, tags).expect("the song folder");
        root
    };
    let bare = folder("bare", "");
    let missing = folder("missing", "#BACKGROUND:gone.png;");
    let later = folder("later", "#BGCHANGES:8=light.png=1=0=0=0;");
    let pictured = folder("pictured", "#BACKGROUND:light.png;");
    // The defaults, the darkest filter at full brightness, brightness
    // off under the default filter.
    let looks = [(0.4, 0.4), (1.0, 1.0), (0.0, 0.4)];
    for look in looks {
        for (name, root) in [("bare", &bare), ("missing", &missing)] {
            for (t, outside, inside) in play(&adapter, root, name, look, &[1.0, 5.0]) {
                for got in outside.into_iter().chain(inside) {
                    assert_eq!(got, GROUND, "{name} {look:?} at {t:.2} s");
                }
            }
        }
        // Brightness off: a background is not drawn, nor its band.
        let shown = look.0 > 0.0;
        let samples = play(&adapter, &pictured, "pictured", look, &[1.0, 5.0]);
        let samples = samples
            .into_iter()
            .chain(play(&adapter, &later, "later", look, &[1.0, 5.0]));
        for (k, (t, outside, inside)) in samples.enumerate() {
            // Before its first change (at 4 s), `later` has none.
            let background = shown && (k < 2 || t > 4.0);
            for (o, i) in outside.into_iter().zip(inside) {
                if background {
                    let darker = i.iter().zip(o).all(|(i, o)| *i < o);
                    assert!(
                        o != GROUND && darker,
                        "{look:?} {k} at {t:.2} s: {o:?} {i:?}"
                    );
                } else {
                    assert_eq!((o, i), (GROUND, GROUND), "{look:?} {k} at {t:.2} s");
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
