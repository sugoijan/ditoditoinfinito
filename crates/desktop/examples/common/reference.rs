//! What the reference examples share: a skin read through the actual
//! loader, its images uploaded, quads drawn offscreen and saved as a PNG.
#![allow(dead_code)]

use ddi_chart::Layout;
use ddi_desktop::library::noteskins::DiskSkin;
use ddi_desktop::play::skins::Prepared;
use ddi_engine::frame::Frame;
use ddi_noteskin::lua::Env;
use ddi_render::material::MaterialParams;
use ddi_render::textured::{TexQuad, TexturedPipeline};
use ddi_render::{FieldGeometry, noteskin::NoteLayer};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// `name`'s value, else `default`.
pub fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Whether `name` is set.
pub fn flag(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub pipeline: TexturedPipeline,
}

impl Gpu {
    pub fn new() -> Result<Gpu> {
        let instance = ddi_desktop::gpu::instance();
        let adapter = ddi_desktop::gpu::adapter(&instance, None).ok_or("GPU required")?;
        let (device, queue) = ddi_desktop::gpu::device(&adapter)?;
        let pipeline = TexturedPipeline::new(&device, wgpu::TextureFormat::Rgba8Unorm, None, false);
        Ok(Gpu {
            device,
            queue,
            pipeline,
        })
    }

    /// `ready`'s images uploaded and named to its play.
    pub fn upload(&mut self, ready: &mut Prepared) {
        for (i, image) in ready.images.iter().enumerate() {
            if let Some(image) = image {
                let [w, h] = image.size;
                let texture = self
                    .pipeline
                    .create_texture(&self.device, w, h, false, false);
                self.queue.write_texture(
                    texture.as_image_copy(),
                    &image.data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w * 4),
                        rows_per_image: Some(h),
                    },
                    texture.size(),
                );
                let id = self.pipeline.add_texture(&self.device, &texture);
                ready.play.set_texture(i, id);
            }
        }
    }

    /// The skin's materials named in `quads` and set at `frame`'s clocks
    /// (`params`: its slots), its images packed as play packs them unless
    /// `DDI_REFERENCE_NO_ATLAS`.
    pub fn materials(
        &mut self,
        ready: &Prepared,
        quads: &mut [TexQuad],
        params: &[MaterialParams],
        frame: &Frame,
    ) {
        self.pipeline
            .name_skin_materials(&self.device, ready.play.skin_materials(), quads);
        if !flag("DDI_REFERENCE_NO_ATLAS") {
            let mut packing = self.device.create_command_encoder(&Default::default());
            self.pipeline
                .pack(&self.device, &mut packing, ready.play.atlas_textures());
            self.queue.submit(Some(packing.finish()));
        }
        self.pipeline
            .set_materials(ddi_render::material::MaterialFrame {
                params,
                beat: frame.beat as f32,
                seconds: frame.song_time as f32,
                linear: false,
            });
    }

    /// `quads` drawn over `clear` into a `size` target, saved to `output`:
    /// through the stencil if `masked`, else (`None`) if some quads use it.
    pub fn capture(
        &mut self,
        quads: &[TexQuad],
        [w, h]: [u32; 2],
        clear: wgpu::Color,
        masked: Option<bool>,
        output: &str,
    ) -> Result<()> {
        let device = &self.device;
        self.pipeline
            .prepare(device, &self.queue, w as f32, h as f32, quads);
        let masked = masked.unwrap_or(self.pipeline.masked());
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
        let mut stencil = ddi_render::stencil::Attachment::default();
        let stencil_view = stencil.view(device, [w, h]);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("reference"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: masked
                    .then(|| ddi_render::stencil::Attachment::pass(stencil_view, true)),
                ..Default::default()
            });
            self.pipeline
                .draw(&mut pass, 0..quads.len() as u32, false, masked);
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
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let data = slice.get_mapped_range()?;
        image::save_buffer(output, &data, w, h, image::ColorType::Rgba8)?;
        Ok(())
    }
}

/// The ground Infinito's references are drawn over.
pub const GROUND: wgpu::Color = wgpu::Color {
    r: 0.0185,
    g: 0.0203,
    b: 0.0383,
    a: 1.0,
};

/// Infinito's ten buttons: 9B's lanes and pump's center at column 9.
pub fn ten_lanes() -> Layout {
    let mut layout = Layout::builtin("danoni-9B").unwrap();
    let mut center = Layout::builtin("pump-single").unwrap().lanes[2].clone();
    center.column = 9.0;
    layout.lanes.push(center);
    layout
}

/// Infinito read from `DDI_REFERENCE_SKIN` (else the bundled folder) over
/// the bundled skins, for `layout`, its Lua told `env`; drawn encoded,
/// like the SVG reference and material fixtures.
pub fn infinito(layout: &Layout, env: Env) -> Result<Prepared> {
    let custom = DiskSkin {
        game: "dance".into(),
        name: "Infinito".into(),
        dir: std::env::var_os("DDI_REFERENCE_SKIN")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/noteskins/dance/Infinito")
            }),
        bundled: false,
        variant_of: None,
    };
    let key = custom.key();
    let mut skins = vec![custom];
    skins.extend(ddi_desktop::library::noteskins::bundled());
    let mut ready = ddi_desktop::play::skins::prepare_authoring(&key, layout, env, &skins)
        .ok_or("skin load failed")?;
    if let Some(e) = ready.load_error.take() {
        return Err(e.into());
    }
    ready.play.set_srgb(false);
    Ok(ready)
}

/// The demo's frame at `DDI_HOLD_BEAT` (0.3), `DDI_REFERENCE_TIME` (half
/// the beat) and `DDI_REFERENCE_WINDOW` (a roll's 0.35 s window).
pub fn frame() -> Result<Frame> {
    let mut demo = ddi_engine::demo::Demo::new("dance-single", 120.0, 1.0).ok_or("demo")?;
    let mut frame = demo.step(0.0).frame;
    frame.beat = env("DDI_HOLD_BEAT", 0.3);
    frame.song_time = env("DDI_REFERENCE_TIME", frame.beat / 2.0);
    frame.roll_window_seconds = env("DDI_REFERENCE_WINDOW", 0.35);
    Ok(frame)
}

/// 96-pixel lanes from x=160 on a `size` field, the receptors at 160 from
/// the top, or from the bottom with `DDI_HOLD_REVERSE`.
pub fn geometry([w, h]: [u32; 2]) -> FieldGeometry {
    let reverse = flag("DDI_HOLD_REVERSE");
    FieldGeometry {
        arrow: 96.0,
        left: 160.0,
        receptor_y: if reverse { h as f32 - 160.0 } else { 160.0 },
        direction: if reverse { -1.0 } else { 1.0 },
        width: w as f32,
        height: h as f32,
    }
}
