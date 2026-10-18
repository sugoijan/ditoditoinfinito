//! The song's picture dimmed by the backgrounds' brightness reads back
//! the same from the play's backdrop (`ddi_render::background`, on an
//! sRGB target and on a plain one) and from the 2D pass's tint (the start
//! prompt's picture, `ddi_ui::screens::prompt`): the brightness multiplies
//! the encoded values everywhere; and the field filter's band over it
//! (the play's backdrop shaders, the prompt's dark quad) alike. Skipped
//! where there is no GPU adapter (CI).

use ddi_desktop::gpu;
use ddi_render::background::{Backdrop, BackgroundPipeline};
use ddi_render::draw2d::DrawList;
use ddi_render::{Painter, TextureKey};

const SIZE: [u32; 2] = [64, 16];
/// The image's stripes, encoded values: greys and a color.
const STRIPES: [[u8; 3]; 4] = [
    [40, 40, 40],
    [128, 128, 128],
    [230, 230, 230],
    [250, 140, 30],
];

fn image() -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..SIZE[1] {
        for x in 0..SIZE[0] {
            let [r, g, b] = STRIPES[(x / 16) as usize];
            out.extend([r, g, b, 255]);
        }
    }
    out
}

fn target(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
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
    })
}

fn write(queue: &wgpu::Queue, texture: &wgpu::Texture) {
    queue.write_texture(
        texture.as_image_copy(),
        &image(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE[0] * 4),
            rows_per_image: Some(SIZE[1]),
        },
        wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
    );
}

/// `texture` read back: RGBA rows.
fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE[0] * 4 * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
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
    slice.get_mapped_range().expect("mapped").to_vec()
}

/// The play's backdrop at `brightness` on a target of `format`.
fn play(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    srgb: bool,
    brightness: f32,
    band: Option<([f32; 2], f32)>,
) -> Vec<u8> {
    let format = if srgb {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    let mut pipeline = BackgroundPipeline::new(device, format, srgb);
    let texture = pipeline.create_texture(device, SIZE[0], SIZE[1]);
    write(queue, &texture);
    let id = pipeline.add_texture(device, &texture);
    let prepared = pipeline.prepare(
        device,
        queue,
        SIZE[0] as f32,
        SIZE[1] as f32,
        brightness,
        Backdrop::image(id),
        None,
        band,
    );
    let out = target(device, format);
    let view = out.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    pipeline.draw_extension(&mut encoder, &prepared);
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
        pipeline.draw(&mut pass, &prepared);
    }
    queue.submit(Some(encoder.finish()));
    read(device, queue, &out)
}

/// The 2D pass's quad of the image tinted `brightness`.
fn prompt(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    brightness: f32,
    band: Option<([f32; 2], f32)>,
) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut painter = Painter::new(device, queue, format);
    let id = painter.texture(TextureKey {
        source: "image".into(),
        max_side: 0,
    });
    painter.insert_texture(device, id, SIZE, |t| write(queue, t));
    let mut list = DrawList::new();
    list.image(
        [0.0, 0.0, SIZE[0] as f32, SIZE[1] as f32],
        id,
        [brightness, brightness, brightness, 1.0],
    );
    // The prompt's filter: a dark quad, its alpha what the band removes.
    if let Some(([x0, x1], keep)) = band {
        list.rect([x0, 0.0, x1, SIZE[1] as f32], [0.0, 0.0, 0.0, 1.0 - keep]);
    }
    let out = target(device, format);
    let view = out.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    painter.draw(
        device,
        queue,
        &mut encoder,
        &view,
        SIZE,
        1.0,
        Some(wgpu::Color::BLACK),
        &list,
        &mut |_, _| {},
    );
    queue.submit(Some(encoder.finish()));
    painter.submitted();
    read(device, queue, &out)
}

#[test]
fn the_prompts_picture_is_as_bright_as_the_plays() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    // The band over the middle two stripes, keeping 60 %.
    let band = Some(([16.0, 48.0], 0.6));
    for brightness in [0.4, 0.75] {
        for band in [None, band] {
            let p = prompt(&device, &queue, brightness, band);
            for srgb in [true, false] {
                let s = play(&device, &queue, srgb, brightness, band);
                for stripe in 0..4u32 {
                    let at = ((8 * SIZE[0] + stripe * 16 + 8) * 4) as usize;
                    let keep = if band.is_some() && (1..3).contains(&stripe) {
                        0.6
                    } else {
                        1.0
                    };
                    for c in 0..3 {
                        let expect =
                            (f32::from(STRIPES[stripe as usize][c]) * brightness * keep).round();
                        for (what, v) in [("prompt", p[at + c]), ("play", s[at + c])] {
                            assert!(
                                (f32::from(v) - expect).abs() <= 2.0,
                                "{what} srgb {srgb} at {brightness}, band {band:?}, stripe {stripe} channel {c}: {v} not {expect}"
                            );
                        }
                    }
                }
            }
        }
    }
}
