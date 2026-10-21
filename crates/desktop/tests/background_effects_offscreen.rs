//! StepMania's named background effects and transitions drawn offscreen
//! (`ddi_library::backgrounds::effects`, `docs/plans/overlays.md`):
//! placements, tints, the additive blend and a wipe, read back as pixels
//! on a plain and an sRGB target. Skipped without a GPU.

use ddi_actor::background::Reveal;
use ddi_desktop::gpu;
use ddi_library::backgrounds::{BgDraw, LayerDraws, Place};
use ddi_render::background::{Backdrop, BackgroundPipeline};

/// The target; a 4:3 frame on it is `[64, 0, 384, 288]`.
const SIZE: [u32; 2] = [512, 288];
const FRAME: [f32; 4] = [64.0, 0.0, 384.0, 288.0];

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

/// A `w × h` image of one color.
fn solid(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &mut BackgroundPipeline,
    [w, h]: [u32; 2],
    rgba: [u8; 4],
) -> usize {
    let texture = pipeline.create_texture(device, w, h);
    let data: Vec<u8> = (0..w * h).flat_map(|_| rgba).collect();
    queue.write_texture(
        texture.as_image_copy(),
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        texture.size(),
    );
    pipeline.add_texture(device, &texture)
}

/// The first layer drawing `draws`, back to front.
fn first(draws: &[BgDraw]) -> Backdrop {
    let mut layer = LayerDraws::default();
    for (slot, d) in layer.iter_mut().zip(draws) {
        *slot = Some(*d);
    }
    Backdrop {
        first: layer,
        ..Backdrop::default()
    }
}

fn scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &mut BackgroundPipeline,
    backdrop: Backdrop,
    srgb: bool,
) -> Vec<u8> {
    let prepared = pipeline.prepare(
        device,
        queue,
        SIZE[0] as f32,
        SIZE[1] as f32,
        1.0,
        backdrop,
        Some(4.0 / 3.0),
        None,
        Reveal::default(),
    );
    let format = if srgb {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    let out = target(device, format);
    let view = out.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    pipeline.draw_extension(&mut encoder, &prepared);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pipeline.draw(&mut pass, &prepared);
    }
    queue.submit(Some(encoder.finish()));
    read(device, queue, &out)
}

/// The pixel at `(x, y)` of the frame, in its `0..1`.
fn at(pixels: &[u8], x: f32, y: f32) -> [u8; 3] {
    let px = (FRAME[0] + x * FRAME[2]) as usize;
    let py = ((FRAME[1] + y * FRAME[3]) as usize).min(SIZE[1] as usize - 1);
    let i = (py * SIZE[0] as usize + px) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 3)
}

#[test]
fn effects_place_tint_add_and_wipe_backgrounds() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    for srgb in [false, true] {
        let format = if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let mut pipeline = BackgroundPipeline::new(&device, format, srgb);
        let red = solid(&device, &queue, &mut pipeline, [640, 480], [255, 0, 0, 255]);
        let green = solid(&device, &queue, &mut pipeline, [320, 240], [0, 255, 0, 255]);
        let grey = solid(
            &device,
            &queue,
            &mut pipeline,
            [64, 64],
            [128, 128, 128, 255],
        );

        // `Centered`: a 320 × 240 image at its own size is the middle
        // half of StepMania's 640 × 480 screen, black around it.
        let centered = BgDraw {
            place: Place::Native { center: [0.5, 0.5] },
            ..BgDraw::cover(green)
        };
        let p = scene(&device, &queue, &mut pipeline, first(&[centered]), srgb);
        assert!(
            near(at(&p, 0.5, 0.5), [0, 255, 0]),
            "{:?}",
            at(&p, 0.5, 0.5)
        );
        assert!(near(at(&p, 0.3, 0.3), [0, 255, 0]));
        assert!(near(at(&p, 0.2, 0.2), [0, 0, 0]), "{:?}", at(&p, 0.2, 0.2));

        // A tint multiplies the encoded colors: grey at half red.
        let tinted = BgDraw {
            color: [0.5, 1.0, 0.0, 1.0],
            ..BgDraw::cover(grey)
        };
        let p = scene(&device, &queue, &mut pipeline, first(&[tinted]), srgb);
        assert!(
            near(at(&p, 0.5, 0.5), [64, 128, 0]),
            "{:?}",
            at(&p, 0.5, 0.5)
        );

        // `OverlayAdd`: green added over red is yellow.
        let added = BgDraw {
            add: true,
            ..BgDraw::cover(green)
        };
        let p = scene(
            &device,
            &queue,
            &mut pipeline,
            first(&[BgDraw::cover(red), added]),
            srgb,
        );
        assert!(
            near(at(&p, 0.5, 0.5), [255, 255, 0]),
            "{:?}",
            at(&p, 0.5, 0.5)
        );

        // `FadeDown` halfway: the red one leaving shows below its edge,
        // the green one coming in above it, the soft edge between.
        let wiping = BgDraw {
            crop: [0.0, 0.5, 0.0, 0.0],
            fade: [0.0, 0.3, 0.0, 0.0],
            ..BgDraw::cover(red)
        };
        let p = scene(
            &device,
            &queue,
            &mut pipeline,
            first(&[BgDraw::cover(green), wiping]),
            srgb,
        );
        assert!(near(at(&p, 0.5, 0.25), [0, 255, 0]));
        assert!(near(at(&p, 0.5, 0.95), [255, 0, 0]));
        let edge = at(&p, 0.5, 0.65);
        assert!(edge[0] > 40 && edge[1] > 40, "{edge:?}");

        // A quarter cropped to cover it: only the top right is red.
        let quarter = BgDraw {
            place: Place::CropTo {
                center: [0.75, 0.25],
                size: [0.5, 0.5],
                flip: true,
            },
            ..BgDraw::cover(red)
        };
        let p = scene(&device, &queue, &mut pipeline, first(&[quarter]), srgb);
        assert!(near(at(&p, 0.75, 0.25), [255, 0, 0]));
        assert!(near(at(&p, 0.25, 0.75), [0, 0, 0]));
    }
}
