//! A play drawn on this machine's GPU as the window draws it (an sRGB
//! view of a plain target, skins through the plain one): frames of an
//! autoplayed play on a fake clock
//! with Dito (SVG, the default skin), a Lua skin (Delta) and the built-in
//! arrows; the song's background behind the field, receptors and notes
//! in it, the skin's textures loaded. Skipped where there is no GPU
//! adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures.

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::gpu;
use ddi_desktop::play::view::{Formats, Rgba, View, render_options};
use ddi_desktop::play::{Feed, Play, Request, host, load};
use ddi_platform::HostTime;
use ddi_ui::state::settings::Settings;

const SIZE: [u32; 2] = [1280, 720];

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn target(g: &Gpu) -> wgpu::Texture {
    g.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("play"),
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    })
}

fn read(g: &Gpu, texture: &wgpu::Texture, mut encoder: wgpu::CommandEncoder) -> Vec<u8> {
    let [w, h] = SIZE;
    let buffer = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(w * 4 * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
        },
        texture.size(),
    );
    g.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = g.device.poll(wgpu::PollType::wait_indefinitely());
    slice.get_mapped_range().expect("mapped").to_vec()
}

fn keep(pixels: &[u8], name: &str) {
    let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") else {
        return;
    };
    let mut out = format!("P6\n{} {}\n255\n", SIZE[0], SIZE[1]).into_bytes();
    for px in pixels.chunks(4) {
        out.extend_from_slice(&px[..3]);
    }
    let _ = std::fs::write(
        std::path::Path::new(&dir).join(format!("play-{name}.ppm")),
        out,
    );
}

fn px(p: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * SIZE[0] + x) * 4) as usize;
    [p[i], p[i + 1], p[i + 2]]
}

/// Pixels in the rectangle that are not near-black.
fn lit(p: &[u8], [x0, y0, x1, y1]: [u32; 4]) -> usize {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if px(p, x, y).iter().any(|&c| c > 40) {
                n += 1;
            }
        }
    }
    n
}

/// Draws chart `chart` of `song` with `settings` at song second `at`;
/// the skin's name.
fn frames(
    g: &Gpu,
    settings: &Settings,
    song: &str,
    chart: usize,
    at: f64,
    srgb_view: bool,
) -> (Vec<u8>, String) {
    let loaded = Rc::new(load::load(song, 48_000).unwrap());
    let formats = Formats::of(wgpu::TextureFormat::Rgba8Unorm, srgb_view);
    let mut view = View::new(&g.device, &g.queue, formats);
    view.set_song(&loaded, settings);
    view.set_noteskin(
        &g.device,
        &g.queue,
        host::noteskin(&loaded, chart, settings),
    );
    let skin = view
        .noteskin()
        .map_or("built-in".to_string(), |n| n.name.clone());
    for (what, source, side) in View::images_to_read(&loaded, settings) {
        let (size, data) = ddi_desktop::images::load(&source, side).expect("the song's picture");
        view.add_background(what, Rgba { size, data });
    }
    view.start();
    let texture = target(g);
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());
    // The play, autoplayed on a fake clock up to `at`.
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let mut play = Play::start(
        Request::song(song, chart, true),
        loaded,
        settings,
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )
    .unwrap();
    let render = render_options(settings);
    let step = 1_000_000_000 / 60;
    let pixels = loop {
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        let now = HostTime(now as f64 / 1e9);
        play.tick(now, None);
        let t = play.session.player.clock().heard_now(now);
        // Every frame is drawn (the skin's commands run per frame); the
        // last one read.
        let mut encoder = g.device.create_command_encoder(&Default::default());
        view.draw(
            &g.device,
            &g.queue,
            &mut encoder,
            if srgb_view { &srgb } else { &plain },
            srgb_view.then_some(&plain),
            SIZE,
            &play,
            render,
            now,
        );
        if t >= at {
            break read(g, &texture, encoder);
        }
        g.queue.submit(Some(encoder.finish()));
    };
    (pixels, skin)
}

#[test]
fn a_play_draws_its_field_skin_and_background() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let g = Gpu { device, queue };
    for (skin, want) in [
        // Dito in the default note colours' variant.
        (None, "Dito Vivid"),
        (Some("bundled:dance/delta"), "Delta"),
        (Some("builtin"), "built-in"),
    ] {
        let settings = Settings {
            noteskin: skin.map(str::to_string),
            ..Settings::default()
        };
        // Beatucada's Challenge chart, 12 s in: notes on screen.
        let (play, name) = frames(&g, &settings, "beatucada", 0, 12.0, true);
        keep(&play, want);
        assert_eq!(name, want);
        // The field: four lanes in the middle, receptors near the top.
        let field = lit(&play, [SIZE[0] / 2 - 160, 60, SIZE[0] / 2 + 160, 160]);
        assert!(field > 2000, "{want}: receptors {field}");
        // Notes further down.
        let notes = lit(&play, [SIZE[0] / 2 - 160, 200, SIZE[0] / 2 + 160, SIZE[1]]);
        assert!(notes > 2000, "{want}: notes {notes}");
        // The background beside the field.
        let side = lit(&play, [0, 0, 300, SIZE[1]]);
        assert!(side > 20_000, "{want}: background {side}");
    }
}

/// A surface with no sRGB view (a Vulkan driver without mutable swapchain
/// formats): the play draws into the plain target itself, its colours
/// encoded by its pipelines, and looks as through the sRGB view.
#[test]
fn a_plain_surface_draws_the_same_play() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let g = Gpu { device, queue };
    let settings = Settings {
        noteskin: Some("builtin".into()),
        ..Settings::default()
    };
    let (viewed, _) = frames(&g, &settings, "beatucada", 0, 12.0, true);
    let (plain, _) = frames(&g, &settings, "beatucada", 0, 12.0, false);
    keep(&plain, "plain-surface");
    let n = viewed.len() as f64;
    let mean = viewed
        .iter()
        .zip(&plain)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / n;
    assert!(mean < 3.0, "mean difference {mean:.2}");
    let field = lit(&plain, [SIZE[0] / 2 - 160, 60, SIZE[0] / 2 + 160, 160]);
    assert!(field > 2000, "receptors {field}");
}
