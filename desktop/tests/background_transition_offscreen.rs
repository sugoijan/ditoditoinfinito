//! Continuity between the menu's cached Gaussian and the live play base.
use ddi_actor::background::{GROUND, Reveal};
use ddi_desktop::gpu;
use ddi_render::background::{Backdrop, BackgroundPipeline};
use ddi_render::{DrawList, Painter, TextureKey, draw2d::Quad};
const SIZE: [u32; 2] = [512, 288];
const IMAGE: [u32; 2] = [256, 192];
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

fn image() -> Vec<u8> {
    (0..IMAGE[1])
        .flat_map(|y| {
            (0..IMAGE[0]).flat_map(move |x| {
                let edge = if x < 128 { 230 } else { 30 };
                [edge, y as u8, (x / 2) as u8, 255]
            })
        })
        .collect()
}
fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, rgba: &[u8]) {
    queue.write_texture(
        texture.as_image_copy(),
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(IMAGE[0] * 4),
            rows_per_image: Some(IMAGE[1]),
        },
        texture.size(),
    );
}
fn scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &mut BackgroundPipeline,
    backdrop: Backdrop,
    reveal: Reveal,
    srgb: bool,
) -> Vec<u8> {
    let prepared = pipeline.prepare(
        device,
        queue,
        SIZE[0] as f32,
        SIZE[1] as f32,
        0.75,
        backdrop,
        Some(4.0 / 3.0),
        None,
        reveal,
    );
    let out = target(
        device,
        if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
    );
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
fn menu(device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
    let mut painter = Painter::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    let id = painter.texture(TextureKey::blurred("image", 256.0));
    painter.insert_rgba(device, queue, id, IMAGE, &image());
    let mut list = DrawList::new();
    list.quad(Quad {
        texture: Some(id),
        cover: true,
        ..Quad::plain(
            [0.0, 0.0, SIZE[0] as f32, SIZE[1] as f32],
            [1.0, 1.0, 1.0, 0.2],
        )
    });
    let out = target(device, wgpu::TextureFormat::Rgba8Unorm);
    let mut encoder = device.create_command_encoder(&Default::default());
    painter.draw(
        device,
        queue,
        &mut encoder,
        &out.create_view(&Default::default()),
        SIZE,
        1.0,
        Some(wgpu::Color {
            r: GROUND[0] as f64,
            g: GROUND[1] as f64,
            b: GROUND[2] as f64,
            a: 1.0,
        }),
        &list,
        &mut |_, _| {},
    );
    queue.submit(Some(encoder.finish()));
    painter.submitted();
    read(device, queue, &out)
}
#[test]
fn base_matches_menu_and_tracks_changes_without_reblurring_static_frames() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let menu = menu(&device, &queue);
    for srgb in [false, true] {
        let mut pipeline = BackgroundPipeline::new(
            &device,
            if srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            srgb,
        );
        let texture = pipeline.create_texture(&device, IMAGE[0], IMAGE[1]);
        write(&queue, &texture, &image());
        let id = pipeline.add_texture(&device, &texture);
        let backdrop = Backdrop::image(id);
        let base = scene(
            &device,
            &queue,
            &mut pipeline,
            backdrop,
            Reveal {
                position: 0.0,
                clarity: 0.0,
            },
            srgb,
        );
        let max_error = menu
            .iter()
            .zip(&base)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(
            max_error <= 2,
            "menu/base crop, tint and Gaussian differ: {max_error}, srgb={srgb}"
        );
        assert_eq!(pipeline.filter_jobs(), [1, 1]);
        let prompt = scene(
            &device,
            &queue,
            &mut pipeline,
            backdrop,
            Reveal {
                position: 1.0,
                clarity: 0.0,
            },
            srgb,
        );
        assert_eq!(
            pipeline.filter_jobs(),
            [1, 1],
            "geometry changes reuse source filters"
        );
        let play = scene(
            &device,
            &queue,
            &mut pipeline,
            backdrop,
            Reveal::default(),
            srgb,
        );
        assert_eq!(pipeline.filter_jobs(), [1, 1]);
        for y in 0..SIZE[1] as usize {
            for x in [16usize, 496] {
                let at = (y * SIZE[0] as usize + x) * 4;
                assert_eq!(
                    &base[at..at + 4],
                    &prompt[at..at + 4],
                    "base does not jump at prompt"
                );
                assert_eq!(
                    &base[at..at + 4],
                    &play[at..at + 4],
                    "base stays blurred during play"
                );
            }
        }
        assert_ne!(prompt, play, "content finishes unblurring");
        let changed: Vec<_> = image()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|c| [c[2], c[0], c[1], 255])
            .collect();
        write(&queue, &texture, &changed);
        pipeline.texture_changed();
        let updated = scene(
            &device,
            &queue,
            &mut pipeline,
            backdrop,
            Reveal::default(),
            srgb,
        );
        assert_ne!(play, updated, "direct RGB frames update the blurred base");
        assert_eq!(pipeline.filter_jobs(), [2, 1]);
        let video = pipeline.add_video(&device, IMAGE[0], IMAGE[1]);
        let luma = (IMAGE[0] * IMAGE[1]) as usize;
        let mut planes = vec![80; luma + luma / 2];
        planes[luma..].fill(128);
        assert!(pipeline.set_video_frame(
            &device, &queue, video, IMAGE[0], IMAGE[1], &planes, false, true
        ));
        let first = scene(
            &device,
            &queue,
            &mut pipeline,
            Backdrop::image(video),
            Reveal::default(),
            srgb,
        );
        planes[..luma].fill(180);
        assert!(pipeline.set_video_frame(
            &device, &queue, video, IMAGE[0], IMAGE[1], &planes, false, true
        ));
        let second = scene(
            &device,
            &queue,
            &mut pipeline,
            Backdrop::image(video),
            Reveal::default(),
            srgb,
        );
        assert_ne!(
            first[..64 * 4],
            second[..64 * 4],
            "YUV movies refresh the ambient base"
        );
        assert_eq!(pipeline.filter_jobs(), [4, 1]);
    }
}
