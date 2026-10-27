//! The 2D pass over a play's sRGB surface (WebGL2 has no plain view of
//! it): through an sRGB target the painter decodes its colors, so a
//! color reads back as it does through a plain one; text drawn the same
//! way lands in the same place. Skipped where there is no GPU adapter
//! (CI).

use ddi_desktop::gpu;
use ddi_render::Painter;
use ddi_render::draw2d::DrawList;

const SIZE: [u32; 2] = [64, 64];

/// `list` drawn on black into a `format` target: RGBA rows as stored.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    list: &DrawList,
) -> Vec<u8> {
    let mut painter = Painter::new(device, queue, format);
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
    painter.submitted();
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    slice.get_mapped_range().expect("mapped").to_vec()
}

#[test]
fn a_color_reads_back_alike_through_srgb_and_plain_targets() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let mut list = DrawList::new();
    // Opaque mid grey, and half of white over black.
    list.rect([0.0, 0.0, 32.0, 64.0], [0.5, 0.5, 0.5, 1.0]);
    list.rect([32.0, 0.0, 64.0, 64.0], [1.0, 1.0, 1.0, 0.5]);
    let plain = draw(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, &list);
    let srgb = draw(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb, &list);
    let at = |p: &[u8], x: u32| p[((32 * SIZE[0] + x) * 4) as usize];
    // The opaque grey is the same encoded value either way.
    assert!(at(&plain, 16).abs_diff(128) <= 1, "{}", at(&plain, 16));
    assert!(at(&srgb, 16).abs_diff(128) <= 1, "{}", at(&srgb, 16));
    // Half of white over black: encoded halfway on the plain target, half
    // the light (brighter, encoded) on the sRGB one.
    assert!(at(&plain, 48).abs_diff(128) <= 1, "{}", at(&plain, 48));
    assert!(at(&srgb, 48).abs_diff(188) <= 2, "{}", at(&srgb, 48));
}

/// Moving/resizing a mask reveals another crop of the same viewport field,
/// instead of carrying its own copy of the texture along with it.
#[test]
fn screen_vivid_is_continuous_across_different_quads() {
    use ddi_actor::Vivid;
    use ddi_render::draw2d::{Affine, Quad};

    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let mut full = Quad::plain([0.0, 0.0, 64.0, 64.0], [1.0; 4]);
    full.shape.vivid = Some(Vivid {
        time: 1.25,
        phase_offset: 0.0,
        evolution_speed: 8.0,
        screen_space: true,
        rect: [-64.0, -64.0, 64.0, 64.0],
    });
    let mut cropped = full.clone();
    cropped.transform = Affine([0.75, 0.0, 0.0, 0.75, 10.0, 14.0]);
    let mut list = DrawList::new();
    list.quad(full);
    let field = draw(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, &list);
    let mut list = DrawList::new();
    list.quad(cropped);
    let crop = draw(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, &list);
    let mut painted = 0;
    for y in 20..50 {
        for x in 20..40 {
            let at = (y * 64 + x) * 4;
            for channel in 0..3 {
                assert!(
                    field[at + channel].abs_diff(crop[at + channel]) <= 2,
                    "field moved with the quad at {x},{y}"
                );
                painted += usize::from(field[at + channel] > 64);
            }
        }
    }
    assert!(painted > 300, "the field must actually draw");
}
