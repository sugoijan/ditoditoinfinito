//! The GPU a test draws on: this machine's adapter or a skip (none on CI,
//! none with `DDI_NO_GPU=1`), and the picture a play or a screen is drawn
//! into and read back from.

use ddi_desktop::gpu;
use ddi_desktop::play::view::Formats;

/// This machine's adapter; `None`, saying so, where there is none (CI
/// runners have none; `DDI_NO_GPU=1` as on them).
pub fn adapter_or_skip() -> Option<wgpu::Adapter> {
    let instance = gpu::instance();
    let adapter = gpu::adapter(&instance, None);
    if adapter.is_none() {
        eprintln!("no GPU adapter: skipped");
    }
    adapter
}

/// The formats a play draws in offscreen: plain RGBA with its sRGB view.
pub fn formats() -> Formats {
    Formats::of(wgpu::TextureFormat::Rgba8Unorm, true)
}

/// An RGBA texture of `size` drawn into through its sRGB view (and its
/// plain one, which a play's 2D pass takes) and read back.
pub struct Target {
    pub texture: wgpu::Texture,
    pub srgb: wgpu::TextureView,
    pub plain: wgpu::TextureView,
    pub size: [u32; 2],
}

impl Target {
    pub fn new(device: &wgpu::Device, size: [u32; 2]) -> Target {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });
        let plain = texture.create_view(&Default::default());
        Target {
            texture,
            srgb,
            plain,
            size,
        }
    }

    /// The pixels drawn so far (RGBA rows), the GPU's work waited for.
    pub fn read(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        gpu::read_rgba(device, queue, &self.texture)
    }
}
