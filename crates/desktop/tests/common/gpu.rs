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

/// A UI test's pictures: its screens drawn on this machine's GPU as the
/// window draws them, or not drawn at all where there is none (CI's
/// runners, `DDI_NO_GPU=1`). What such a test asserts of the room, the
/// screens on top, the actors and their places needs no GPU and runs
/// either way (so CI checks desktop's room wiring); only what reads the
/// pixels sits inside `if let Some(picture) = pictures.draw(…)`.
pub struct Pictures {
    gpu: Option<gpu::Gpu>,
}

/// The format [`Pictures`] draws in.
pub const PICTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Pictures {
    /// On this machine's GPU; without one, saying so once: the pictures
    /// are skipped, the test goes on.
    pub fn new() -> Pictures {
        let instance = gpu::instance();
        let gpu = gpu::adapter(&instance, None)
            .and_then(|adapter| gpu::device(&adapter).ok())
            .map(|(device, queue)| gpu::Gpu::new(device, queue, PICTURE_FORMAT));
        if gpu.is_none() {
            eprintln!("no GPU adapter: the pictures are skipped, the rest runs");
        }
        Pictures { gpu }
    }

    /// Whether pictures are drawn here.
    pub fn drawn(&self) -> bool {
        self.gpu.is_some()
    }

    /// `ui` drawn at `size` and read back (RGBA rows); `None` without a
    /// GPU, the UI brought up to date all the same.
    pub fn draw(&mut self, ui: &mut ddi_ui::Ui, size: [u32; 2]) -> Option<Vec<u8>> {
        match &mut self.gpu {
            Some(gpu) => Some(gpu.draw_offscreen(ui, size, PICTURE_FORMAT)),
            None => {
                ui.update(0.0);
                None
            }
        }
    }

    /// [`Pictures::draw`], kept as `name` under `DDI_OFFSCREEN_PPM`
    /// where that is set.
    pub fn shot(&mut self, ui: &mut ddi_ui::Ui, size: [u32; 2], name: &str) -> Option<Vec<u8>> {
        let picture = self.draw(ui, size)?;
        super::shot(name, &picture, size);
        Some(picture)
    }

    /// The GPU itself, for what a test draws by its own hand.
    pub fn gpu(&mut self) -> Option<&mut gpu::Gpu> {
        self.gpu.as_mut()
    }
}

/// What a test asserts of a picture, where one was drawn: `look` of its
/// pixels; `true` where there is none to look at (no GPU), so the
/// assertion around it holds and the test goes on to what needs none.
pub fn seen(picture: &Option<Vec<u8>>, look: impl FnOnce(&[u8]) -> bool) -> bool {
    picture.as_deref().is_none_or(look)
}

impl Default for Pictures {
    fn default() -> Self {
        Self::new()
    }
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
