//! Cached Gaussian images on the actual GPU: smooth edges, clamped borders,
//! alpha-safe filtering, one job per upload and regeneration after device loss.

use ddi_desktop::gpu;
use ddi_render::{DrawList, Painter, TextureId, TextureKey, draw2d};

const SIZE: [u32; 2] = [128, 64];

fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    painter: &mut Painter,
    id: TextureId,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cached Gaussian test"),
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
        view_formats: &[],
    });
    let mut list = DrawList::new();
    list.quad(draw2d::Quad {
        texture: Some(id),
        ..draw2d::Quad::plain([0.0, 0.0, SIZE[0] as f32, SIZE[1] as f32], [1.0; 4])
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    painter.draw(
        device,
        queue,
        &mut encoder,
        &texture.create_view(&Default::default()),
        SIZE,
        1.0,
        Some(wgpu::Color::BLACK),
        &list,
        &mut |_, _| {},
    );
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE[0] * SIZE[1] * 4),
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
    painter.submitted();
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("GPU completes");
    slice.get_mapped_range().expect("mapped").to_vec()
}

fn image(left: [u8; 4], right: [u8; 4]) -> Vec<u8> {
    (0..SIZE[1])
        .flat_map(|_| (0..SIZE[0]).flat_map(|x| if x < SIZE[0] / 2 { left } else { right }))
        .collect()
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * SIZE[0] as usize + x) * 4..][..4]
}

#[test]
fn gaussian_is_smooth_alpha_safe_and_only_runs_on_upload() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut painter = Painter::new(&device, &queue, format);
    let source = image([255; 4], [0, 0, 0, 255]);
    let id = painter.texture(TextureKey::blurred("contrast", 128.0));
    painter.insert_rgba(&device, &queue, id, SIZE, &source);
    painter.insert_rgba(&device, &queue, id, SIZE, &source);
    let first = draw(&device, &queue, &mut painter, id);
    assert_eq!(pixel(&first, 0, 0), [255; 4], "clamp the border");
    assert_eq!(pixel(&first, 127, 63), [0, 0, 0, 255]);
    assert!((70..180).contains(&pixel(&first, 64, 32)[0]));
    for x in 40..88 {
        assert!(
            pixel(&first, x, 32)[0] >= pixel(&first, x + 1, 32)[0],
            "no ghost edges"
        );
    }
    for _ in 0..3 {
        assert_eq!(
            draw(&device, &queue, &mut painter, id),
            first,
            "no accumulating blur"
        );
    }
    assert_eq!(painter.stats().blurred, 1);
    let sharp = painter.texture(TextureKey::drawn("contrast", 128.0));
    painter.insert_rgba(&device, &queue, sharp, SIZE, &source);
    assert_eq!(
        draw(&device, &queue, &mut painter, sharp),
        source,
        "sharp copy stays sharp"
    );
    assert_eq!(painter.stats().blurred, 1);

    let (device, queue) = gpu::device(&adapter).expect("replacement device");
    painter.device_changed(&device, &queue, format);
    painter.insert_rgba(&device, &queue, id, SIZE, &source);
    assert_eq!(draw(&device, &queue, &mut painter, id), first);
    assert_eq!(painter.stats().blurred, 2, "regenerated on the new device");

    let alpha = image([255, 0, 0, 255], [0, 0, 255, 0]);
    let id = painter.texture(TextureKey::blurred("transparent", 128.0));
    painter.insert_rgba(&device, &queue, id, SIZE, &alpha);
    let filtered = draw(&device, &queue, &mut painter, id);
    let edge = pixel(&filtered, 64, 32);
    assert!((70..180).contains(&edge[0]));
    assert_eq!(
        edge[2], 0,
        "invisible blue must not bleed into the red edge"
    );
}
