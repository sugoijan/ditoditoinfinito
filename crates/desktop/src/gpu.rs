//! The painter (`ddi_render`'s 2D pass) on desktop: over a window's
//! surface, or into a texture for the offscreen test. Text is
//! cosmic-text's with the bundled faces (the HUD's numbers, the theme's
//! Iosevka Term) and the system's fonts behind them (P4). Menus are drawn
//! in encoded values (P11), so the surface's format is a plain one, not
//! sRGB. Images are decoded on other threads when a frame asks for them
//! ([`crate::images::Loader`]); a frame after they are in shows them.

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

/// The bundled faces under the families styles name them by: the HUD's
/// numbers and the theme's text face (the web registers the same files
/// under the same names, `crates/web/src/web/text.rs`).
pub const FACES: &[(&str, &[u8])] = &[
    (
        "Iosevka Custom",
        include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf"),
    ),
    (
        "Iosevka Term",
        include_bytes!("../../../assets/fonts/IosevkaTerm-Regular-latin.ttf"),
    ),
    (
        "Iosevka Term",
        include_bytes!("../../../assets/fonts/IosevkaTerm-SemiBold-latin.ttf"),
    ),
    (
        "Iosevka Term",
        include_bytes!("../../../assets/fonts/IosevkaTerm-Bold-latin.ttf"),
    ),
];

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
    // CI: none, as on a machine without a GPU (a runner's software or
    // virtual adapter would run the GPU tests on pictures they were not
    // checked against).
    if std::env::var_os("DDI_NO_GPU").is_some() {
        return None;
    }
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

/// What is drawn before the UI in a frame ([`Gpu::draw_over`]).
pub type Scene<'a> = dyn FnMut(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder) + 'a;

/// The UI's draw list on the device.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub painter: Painter,
    list: DrawList,
    images: crate::images::Loader,
    /// Options' field preview, where the screen shows it.
    pub field: crate::field_preview::Field,
}

impl Gpu {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat) -> Gpu {
        let mut painter = Painter::new(&device, &queue, format);
        painter.set_rasterizer(Box::new(CosmicRasterizer::with_families(FACES, true)));
        Gpu {
            device,
            queue,
            painter,
            list: DrawList::new(),
            images: crate::images::Loader::new(),
            field: Default::default(),
        }
    }

    /// Whether images are being decoded.
    pub fn loading(&self) -> bool {
        self.images.pending() > 0
    }

    /// Whether decoded images wait for a frame to show them.
    pub fn images_ready(&mut self) -> bool {
        self.images.has_ready()
    }

    /// Puts the images decoded so far on the device (`wait`: every one
    /// asked for); how many. What failed keeps its placeholder.
    fn receive(&mut self, wait: bool) -> usize {
        let decoded = if wait {
            self.images.wait()
        } else {
            self.images.take()
        };
        let n = decoded.len();
        for (id, image) in decoded {
            match image {
                Some((size, rgba)) => {
                    self.painter
                        .insert_rgba(&self.device, &self.queue, id, size, &rgba)
                }
                None => self.painter.fail_texture(id),
            }
        }
        n
    }

    /// Draws `ui` into `target` (`size` physical pixels), with the images
    /// decoded by now, and submits; images it lacks are asked for.
    pub fn draw(&mut self, ui: &mut Ui, target: &wgpu::TextureView, size: [u32; 2]) {
        self.draw_over(ui, target, size, None);
    }

    /// [`Gpu::draw`] over what `scene` records first into the frame's
    /// encoder (the start prompt's picture: the UI is drawn over it, not
    /// over a clear screen).
    pub fn draw_over(
        &mut self,
        ui: &mut Ui,
        target: &wgpu::TextureView,
        size: [u32; 2],
        scene: Option<&mut Scene<'_>>,
    ) {
        self.receive(false);
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
            let image = ui.texture_source(t)?;
            let key = if image.blurred {
                TextureKey::blurred(image.source, image.side)
            } else {
                TextureKey::drawn(image.source, image.side * scale)
            };
            Some(painter.texture(key))
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ui") });
        // The field preview's frame, before the painter shows it.
        self.field
            .draw(&self.device, &self.queue, &mut encoder, &mut self.painter);
        let clear = match scene {
            Some(scene) => {
                scene(&self.device, &self.queue, &mut encoder);
                None
            }
            None => Some(CLEAR),
        };
        self.painter.draw(
            &self.device,
            &self.queue,
            &mut encoder,
            target,
            size,
            scale,
            clear,
            &self.list,
            &mut |_slot, _encoder| {},
        );
        self.queue.submit(Some(encoder.finish()));
        self.painter.submitted();
        // Images the draw asked for, decoded off this thread.
        let limit = self.painter.max_texture_side();
        for (id, key) in self.painter.take_texture_requests() {
            let side = match key.max_side {
                0 => limit,
                s => s.min(limit),
            };
            self.images.request(id, &key.source, side);
        }
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
        // Again once the images asked for are in.
        for _ in 0..3 {
            self.draw(ui, &view, size);
            if !self.loading() {
                break;
            }
            self.receive(true);
            ui.update(0.0);
        }
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
