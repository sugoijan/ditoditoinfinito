//! The HUD's judgement pop on this machine's GPU: drawn at a few cached
//! sizes (`ddi_render::scene::popped`) it looks as drawn at the exact
//! size of each frame (the same box within a pixel, as much ink), rests
//! exactly, and a pop shapes its sizes once, not each frame. Skipped
//! where there is no GPU adapter (CI).

use ddi_desktop::gpu;
use ddi_render::scene::popped;
use ddi_render::{Align, TextItem, TextPipeline};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");
/// Rows of 1024 bytes, as copies want them.
const SIZE: [u32; 2] = [256, 96];

fn item(size: f32) -> TextItem {
    TextItem {
        text: "Fantastic".into(),
        x: 128.0,
        y: 20.0,
        size,
        color: [0.4, 0.85, 1.0, 1.0],
        align: Align::Center,
        weight: 800,
    }
}

/// `items` drawn on black: RGBA rows.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    text: &mut TextPipeline,
    items: &[TextItem],
) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
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
    text.prepare(device, queue, SIZE[0], SIZE[1], items)
        .expect("prepared");
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        text.draw(&mut pass);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE[0] * 4 * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE[0] * 4),
                rows_per_image: Some(SIZE[1]),
            },
        },
        texture.size(),
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().expect("mapped").to_vec();
    text.trim();
    data
}

/// The lit pixels' box `[x0, y0, x1, y1]` and their ink (summed green).
fn ink(pixels: &[u8]) -> ([u32; 4], f32) {
    let (mut b, mut sum) = ([u32::MAX, u32::MAX, 0, 0], 0.0);
    for y in 0..SIZE[1] {
        for x in 0..SIZE[0] {
            let g = pixels[((y * SIZE[0] + x) * 4 + 1) as usize];
            if g > 40 {
                b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            }
            sum += f32::from(g);
        }
    }
    (b, sum)
}

#[test]
fn a_judgement_pop_looks_the_same_at_its_cached_sizes() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let mut text = TextPipeline::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, &[FONT]);
    let rest = 50.0f32 * 0.55;
    for t in [0.0f32, 0.3, 0.55, 0.8, 1.0] {
        let exact = rest * (1.0 + 0.15 * (1.0 - t));
        let size = popped(rest, exact);
        let (a, ink_a) = ink(&draw(&device, &queue, &mut text, &[item(exact)]));
        let (b, ink_b) = ink(&draw(&device, &queue, &mut text, &[item(size)]));
        for k in 0..4 {
            assert!(a[k].abs_diff(b[k]) <= 1, "{t}: {a:?} {b:?}");
        }
        assert!((ink_a - ink_b).abs() / ink_a < 0.04, "{t}: {ink_a} {ink_b}");
        if t == 1.0 {
            assert_eq!(size, exact, "rests exactly");
        }
    }
    // Two pops at 144 frames a second: the second shapes nothing.
    let pop = |text: &mut TextPipeline| {
        for f in 0..=20 {
            let age = f as f32 / 144.0;
            let exact = rest * (1.0 + 0.15 * (1.0 - (age / 0.12).min(1.0)));
            draw(&device, &queue, text, &[item(popped(rest, exact))]);
        }
    };
    let before = text.shaped();
    pop(&mut text);
    let first = text.shaped() - before;
    pop(&mut text);
    let second = text.shaped() - before - first;
    assert!(first <= 10, "{first}");
    assert_eq!(second, 0);
}
