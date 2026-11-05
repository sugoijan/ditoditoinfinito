//! The start prompt over a song without a background shows its generated
//! background's first frame, held still, as its picture (the maintainer,
//! 2026-11-06; `docs/plans/phase-8-gaps.md`, step 5): drawn by the shell
//! for the 2D pass when it asks (`ddi_shell::generative::Stills`), the
//! kaleidoscope folding the song's jacket where it has one; none with
//! the generated background off. As the window runs it (`play::host`
//! with the UI core's prompt), offscreen on a fake clock with no sound
//! device, song folders made here. Skipped where there is no GPU adapter
//! (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures.

mod common;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::event::UiEvent;
use ddi_ui::state::settings::{GeneratedBackground, Settings};

const SIZE: [u32; 2] = [960, 540];
/// The screen's ground (`ddi_actor::background::GROUND`), encoded.
const GROUND: [u8; 3] = [5, 2, 13];

/// A song folder `name` under `dir` with silent music, and a red jacket
/// (`jacket.png`, found by its name) if `jacket` (any picture in the
/// folder may be taken for its banner or jacket).
fn song(dir: &Path, name: &str, jacket: bool) {
    let song = dir.join("Pack").join(name);
    std::fs::create_dir_all(&song).unwrap();
    std::fs::write(
        song.join("song.sm"),
        format!(
            "#TITLE:{name};#MUSIC:music.wav;#OFFSET:0;#BPMS:0=120;\
             #NOTES:dance-single::Easy:2:0,0,0,0,0:\n0000\n0000\n0000\n1000\n;"
        ),
    )
    .unwrap();
    if jacket {
        image::RgbaImage::from_fn(32, 32, |x, y| {
            image::Rgba([220, 30 + ((x * 7 + y * 3) % 40) as u8, 40, 255])
        })
        .save(song.join("jacket.png"))
        .unwrap();
    }
    let rate = 44_100u32;
    let data = vec![0u8; (rate * 4 * 4) as usize];
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
    std::fs::write(song.join("music.wav"), wav).unwrap();
}

/// The prompt of song `id` with `look`, drawn once ready: what it says
/// its backdrop is, and its pixels.
fn prompt(
    gpu: &mut Gpu,
    id: &str,
    look: GeneratedBackground,
    name: &str,
) -> (Option<String>, Vec<u8>) {
    Settings {
        generated_background: look,
        ..Settings::default()
    }
    .save();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let fold: ddi_ui::dev_widgets::FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_desktop::Nfkd, s));
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    ui.update(0.5);
    let mut host = Host::new(Rc::default(), None);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    *host.requested.borrow_mut() = Some(Request::song(id, 0, true));
    let mut waited = 0;
    loop {
        let dev = Device {
            device: &gpu.device,
            queue: &gpu.queue,
            formats: Formats::of(format, true),
        };
        host.poll(&mut ui, &env, Some(dev), 48_000);
        // As the shell hands it on (`shell.rs`).
        let (source, still) = host.prompt_still();
        gpu.stills.set(source, still);
        ui.update(0.05);
        let pictures = host.prompt_ready();
        let shown = env.state.borrow().backdrop.is_some();
        if pictures && (shown || look == GeneratedBackground::Off) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "{name}: never ready");
    }
    let mut pixels = Vec::new();
    for _ in 0..6 {
        ui.update(1.0 / 60.0);
        pixels = gpu.draw_offscreen(&mut ui, SIZE, format);
    }
    if let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") {
        let mut ppm = format!("P6\n{} {}\n255\n", SIZE[0], SIZE[1]).into_bytes();
        ppm.extend(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]]),
        );
        let _ = std::fs::write(Path::new(&dir).join(format!("prompt-{name}.ppm")), ppm);
    }
    let backdrop = env.state.borrow().backdrop.clone();
    (backdrop, pixels)
}

/// Down the screen's left edge, every 8th pixel: the share that is not
/// the ground, and the mean of each channel.
fn left_edge(pixels: &[u8]) -> (f64, [f64; 3]) {
    let (mut lit, mut n) = (0.0, 0.0);
    let mut sum = [0.0; 3];
    for y in (40..SIZE[1] - 60).step_by(8) {
        for x in (4..140).step_by(8) {
            let i = ((y * SIZE[0] + x) * 4) as usize;
            let p = [pixels[i], pixels[i + 1], pixels[i + 2]];
            if p != GROUND {
                lit += 1.0;
            }
            for (s, v) in sum.iter_mut().zip(p) {
                *s += f64::from(v);
            }
            n += 1.0;
        }
    }
    (lit / n, sum.map(|s| s / n))
}

#[test]
fn the_prompt_shows_the_generated_backgrounds_first_frame() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    storage::install(Rc::new(MemoryStore::default()));
    let dir = std::env::temp_dir().join(format!("ddi-generated-prompt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    song(&dir, "Bare", false);
    song(&dir, "Jacketed", true);
    let (songs, report) = ddi_desktop::library::scan_folders(std::slice::from_ref(&dir));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    ddi_desktop::library::register_for_tests(&songs);
    let id = |title: &str| {
        songs
            .iter()
            .find(|s| s.entry.title == title)
            .map(|s| s.entry.id.clone())
            .unwrap()
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let mut gpu = Gpu::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);

    use GeneratedBackground::*;
    // The field and lines (the default): its still in the picture's
    // place, lit.
    let (backdrop, field) = prompt(&mut gpu, &id("Bare"), Field, "field");
    assert!(
        backdrop
            .as_deref()
            .is_some_and(|b| b.starts_with("generated-still:")),
        "{backdrop:?}"
    );
    let (lit, _) = left_edge(&field);
    assert!(lit > 0.9, "field: {lit}");

    // The kaleidoscope: glass in the palette's blue gray, or the jacket
    // folded, red.
    let (_, glass) = prompt(&mut gpu, &id("Bare"), Kaleidoscope, "glass");
    let (_, [gr, _, gb]) = left_edge(&glass);
    assert!(gb > gr, "glass: {gr} {gb}");
    let (_, jacket) = prompt(&mut gpu, &id("Jacketed"), Kaleidoscope, "jacket");
    let (lit, [jr, _, jb]) = left_edge(&jacket);
    assert!(lit > 0.9 && jr > jb * 1.3, "jacket: {lit} {jr} {jb}");

    // Off: the ground, no picture.
    let (backdrop, off) = prompt(&mut gpu, &id("Bare"), Off, "off");
    assert_eq!(backdrop, None);
    let (lit, _) = left_edge(&off);
    assert!(lit < 0.05, "off: {lit}");
    let _ = std::fs::remove_dir_all(&dir);
}
