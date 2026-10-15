//! The painter (`ddi_render`'s 2D pass) on desktop: over a window's
//! surface, or into a texture for the offscreen test. Text is
//! cosmic-text's with the system's fonts (P4). Menus are drawn in encoded
//! values (P11), so the surface's format is a plain one, not sRGB.

use ddi_render::cosmic::CosmicRasterizer;
use ddi_render::texture_cache::TextureKey;
use ddi_render::{DrawList, Painter};
use ddi_ui::Ui;

/// The developer screen's background (the web's, `components::ui_dev`).
pub const CLEAR: wgpu::Color = wgpu::Color {
    r: 12.0 / 255.0,
    g: 12.0 / 255.0,
    b: 18.0 / 255.0,
    a: 1.0,
};

/// The native backends.
pub fn instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    })
}

pub fn adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Option<wgpu::Adapter> {
    crate::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: surface,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()
}

pub fn device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), String> {
    crate::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("ddi-desktop"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .map_err(|e| format!("request_device failed: {e:?}"))
}

/// The plain (not sRGB) variant of what the surface offers.
pub fn plain_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .or_else(|| formats.first().map(|f| f.remove_srgb_suffix()))
}

/// The UI's draw list on the device.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub painter: Painter,
    list: DrawList,
}

impl Gpu {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat) -> Gpu {
        let mut painter = Painter::new(&device, &queue, format);
        painter.set_rasterizer(Box::new(CosmicRasterizer::new(&[], true)));
        Gpu {
            device,
            queue,
            painter,
            list: DrawList::new(),
        }
    }

    /// Draws `ui` into `target` (`size` physical pixels) and submits.
    pub fn draw(&mut self, ui: &mut Ui, target: &wgpu::TextureView, size: [u32; 2]) {
        let vp = *ui.viewport();
        let scale = vp.scale;
        let shift = [
            1.0,
            0.0,
            0.0,
            1.0,
            vp.offset[0] / scale,
            vp.offset[1] / scale,
        ];
        let drawn = {
            let mut measure = self.painter.measure(scale);
            ui.draw_in(&mut measure, shift)
        };
        self.list.clear();
        // Actor textures to the cache's, by source and the size drawn.
        let painter = &mut self.painter;
        ddi_render::actors2d::append(&drawn, &mut self.list, &mut |t| {
            let (source, side) = ui.texture_source(t)?;
            Some(painter.texture(TextureKey::drawn(source, side * scale)))
        });
        // No image loader on desktop yet (the library on disk comes with
        // phase 9): every image asked for fails, the placeholders show.
        for (id, _) in self.painter.take_texture_requests() {
            self.painter.fail_texture(id);
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ui") });
        self.painter.draw(
            &self.device,
            &self.queue,
            &mut encoder,
            target,
            size,
            scale,
            Some(CLEAR),
            &self.list,
            &mut |_slot, _encoder| {},
        );
        self.queue.submit(Some(encoder.finish()));
        self.painter.submitted();
    }

    /// Draws `ui` into a texture of `size` and reads it back: RGBA rows.
    pub fn draw_offscreen(
        &mut self,
        ui: &mut Ui,
        size: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Vec<u8> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.draw(ui, &view, size);
        let row = size[0] * 4;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded * size[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(size[1]),
                },
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let Ok(data) = slice.get_mapped_range() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity((row * size[1]) as usize);
        for y in 0..size[1] {
            let start = (y * padded) as usize;
            out.extend_from_slice(&data[start..start + row as usize]);
        }
        if format == wgpu::TextureFormat::Bgra8Unorm {
            for px in out.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
        }
        out
    }
}
