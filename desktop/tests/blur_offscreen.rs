//! The 2D pass's motion blur drawn on this machine's GPU: a quad smeared
//! along its blur keeps its color and its coverage, its edges ramped
//! over the blur's length; text likewise. Skipped where there is no GPU
//! adapter (CI).

use ddi_actor::text::TextStyle;
use ddi_desktop::gpu;
use ddi_render::cosmic::CosmicRasterizer;
use ddi_render::{DrawList, Painter, draw2d};

// Rows of 1024 bytes, as copies want them.
const SIZE: [u32; 2] = [256, 200];

/// `list` drawn on black: RGBA rows.
fn draw(device: &wgpu::Device, queue: &wgpu::Queue, list: &DrawList) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut painter = Painter::new(device, queue, format);
    painter.set_rasterizer(Box::new(CosmicRasterizer::with_families(gpu::FACES, true)));
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    painter.draw(
        device,
        queue,
        &mut encoder,
        &view,
        SIZE,
        1.0,
        Some(wgpu::Color::BLACK),
        list,
        &mut |_, _| {},
    );
    let row = SIZE[0] * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE[1]),
            },
        },
        texture.size(),
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    slice.get_mapped_range().expect("mapped").to_vec()
}

fn red(pixels: &[u8], x: u32, y: u32) -> f32 {
    f32::from(pixels[((y * SIZE[0] + x) * 4) as usize]) / 255.0
}

#[test]
fn quads_and_runs_smear_along_their_blur() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    // A white quad 40 high, blurred 20 down its height: a quad of its
    // own units (the unit square scaled), as actors draw them.
    let quad = |blur: [f32; 2]| {
        let mut l = DrawList::new();
        l.quad(draw2d::Quad {
            rect: [-0.5, -0.5, 0.5, 0.5],
            transform: draw2d::Affine::scale(40.0, 40.0)
                .then(draw2d::Affine::translate(100.0, 100.0)),
            blur,
            ..draw2d::Quad::plain([0.0; 4], [1.0; 4])
        });
        l
    };
    let sharp = draw(&device, &queue, &quad([0.0; 2]));
    let smeared = draw(&device, &queue, &quad([0.0, 0.5]));
    // Sharp: 80..120 down.
    assert_eq!(red(&sharp, 100, 79), 0.0);
    assert_eq!(red(&sharp, 100, 80), 1.0);
    assert_eq!(red(&sharp, 100, 119), 1.0);
    assert_eq!(red(&sharp, 100, 120), 0.0);
    // Smeared: ramps 70..90 and 110..130, full between, the same across.
    let near = |y: u32, v: f32| {
        let got = red(&smeared, 100, y);
        assert!((got - v).abs() < 0.11, "at {y}: {got}, not {v}");
    };
    near(68, 0.0);
    near(75, 0.275);
    near(80, 0.525);
    near(85, 0.775);
    near(92, 1.0);
    near(108, 1.0);
    near(120, 0.475);
    near(131, 0.0);
    assert_eq!(red(&smeared, 79, 100), 0.0);
    assert_eq!(red(&smeared, 80, 100), 1.0);
    // As much drawn: the column's coverage kept.
    let column = |p: &[u8]| (0..SIZE[1]).map(|y| red(p, 100, y)).sum::<f32>();
    assert!(
        (column(&smeared) - 40.0).abs() < 1.0,
        "{}",
        column(&smeared)
    );

    // A run: smeared, about as much ink.
    let run = |blur: [f32; 2]| {
        let mut l = DrawList::new();
        l.push(draw2d::Command::Text(draw2d::TextRun {
            text: "DITO".into(),
            style: TextStyle::new("Iosevka Term", 40.0),
            transform: draw2d::Affine::translate(40.0, 60.0),
            color: [1.0; 4],
            blend: draw2d::Blend::Normal,
            mask: draw2d::Mask::None,
            perspective: None,
            blur,
        }));
        l
    };
    let ink = |p: &[u8]| {
        p.as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from(c[0]))
            .sum::<f32>()
    };
    let sharp = draw(&device, &queue, &run([0.0; 2]));
    let smeared = draw(&device, &queue, &run([0.0, 16.0]));
    assert!(
        ink(&sharp) > 300.0 * 255.0,
        "text was drawn: {}",
        ink(&sharp) / 255.0
    );
    let ratio = ink(&smeared) / ink(&sharp);
    assert!((ratio - 1.0).abs() < 0.08, "{ratio}");
    // Spread: rows above and below the sharp run have ink now.
    let rows = |p: &[u8]| {
        (0..SIZE[1])
            .filter(|&y| (0..SIZE[0]).any(|x| red(p, x, y) > 0.02))
            .count()
    };
    assert!(
        rows(&smeared) >= rows(&sharp) + 12,
        "{} {}",
        rows(&smeared),
        rows(&sharp)
    );
}
