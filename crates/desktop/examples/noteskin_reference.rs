//! Capture Peters-DDR Vivid through the actual loader and textured renderer.
//! No window/audio. `cargo run -p ddi-desktop --no-default-features --example
//! noteskin_reference -- OUTPUT.png` (scratch DDI_HOME, GPU required).
use ddi_chart::{Layout, Quantization};
use ddi_engine::frame::{NoteSprite, SpriteKind};
use ddi_noteskin::lua::Env;
use ddi_render::{FieldGeometry, noteskin::NoteLayer, textured::TexturedPipeline};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("OUTPUT.png required")?;
    let instance = ddi_desktop::gpu::instance();
    let adapter = ddi_desktop::gpu::adapter(&instance, None).ok_or("GPU required")?;
    let (device, queue) = ddi_desktop::gpu::device(&adapter)?;
    let layout = Layout::dance_single();
    let mut ready = ddi_desktop::play::skins::prepare_authoring(
        "bundled:dance/peters-ddr-vivid",
        &layout,
        Env::default(),
        &ddi_desktop::library::noteskins::bundled(),
    )
    .ok_or("skin load failed")?;
    if let Some(e) = ready.load_error {
        return Err(e.into());
    }
    let mut pipeline = TexturedPipeline::new(&device, wgpu::TextureFormat::Rgba8Unorm, None, false);
    for (i, image) in ready.images.iter().enumerate() {
        if let Some(image) = image {
            let [w, h] = image.size;
            let texture = pipeline.create_texture(&device, w, h, false, false);
            queue.write_texture(
                texture.as_image_copy(),
                &image.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                texture.size(),
            );
            let id = pipeline.add_texture(&device, &texture);
            ready.play.set_texture(i, id);
        }
    }
    let (w, h) = (2048u32, 1024u32);
    let mut quads = Vec::new();
    let mut demo = ddi_engine::demo::Demo::new("dance-single", 120.0, 1.0).ok_or("demo")?;
    let mut frame = demo.step(0.0).frame;
    frame.notes = vec![NoteSprite {
        note_index: 0,
        lane: 1,
        y: 0.,
        kind: SpriteKind::Tap,
        quantization: Quantization::N4,
        beat_frac: 0.,
        color: None,
        alpha: 1.,
        glow: 0.,
        beat: 0.,
        hold_life: 1.,
    }];
    for index in 0..128 {
        frame.beat = index as f64 / 32.;
        frame.song_time = frame.beat / 2.;
        ready.play.update(&frame, std::iter::empty(), false);
        let geo = FieldGeometry {
            arrow: 128.,
            left: (index % 16) as f32 * 128. - 128.,
            receptor_y: (index / 16) as f32 * 128. + 64.,
            direction: 1.,
            width: w as f32,
            height: h as f32,
        };
        let mut receptor = Vec::new();
        ready.play.receptors(&frame, &layout, &geo, &mut receptor);
        ready.play.notes(&frame, &layout, &geo, &mut quads);
    }
    pipeline.prepare(&device, &queue, w as f32, h as f32, &quads);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reference atlas"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("reference"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pipeline.draw(&mut pass, 0..quads.len() as u32, false, false);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("reference readback"),
        size: (w * h * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
        },
        target.size(),
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely())?;
    let data = slice.get_mapped_range()?;
    image::save_buffer(output, &data, w, h, image::ColorType::Rgba8)?;
    eprintln!("captured 128 frames / four beats / original model + textures");
    Ok(())
}
