//! Device-neutral NV12 textures. Native ownership remains in the shell.
use super::*;
impl BackgroundPipeline {
    /// Caller retains the imported planes and their external owner through GPU
    /// completion. The renderer stores only ordinary wgpu texture references.
    pub fn set_video_nv12(
        &mut self,
        device: &wgpu::Device,
        id: usize,
        y: &wgpu::Texture,
        uv: &wgpu::Texture,
        bt709: bool,
        full_range: bool,
    ) {
        assert_eq!(y.format(), wgpu::TextureFormat::R8Unorm);
        assert_eq!(uv.format(), wgpu::TextureFormat::Rg8Unorm);
        assert_eq!(
            [uv.width(), uv.height()],
            [y.width().div_ceil(2), y.height().div_ceil(2)]
        );
        self.revision = self.revision.wrapping_add(1);
        let image = &mut self.images[id];
        let yv = y.create_view(&Default::default());
        let cv = uv.create_view(&Default::default());
        image.group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-video-nv12-bind"),
            layout: &self.yuv_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: Self::params_binding(&self.params),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&yv),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&cv),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&cv),
                },
            ],
        });
        image.width = y.width() as f32;
        image.height = y.height() as f32;
        image.video = Some(Video {
            planes: [y.clone(), uv.clone(), uv.clone()],
            bt709,
            full_range,
            nv12: true,
        });
    }
}
