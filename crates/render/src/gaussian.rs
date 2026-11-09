//! A two-pass Gaussian filter for small cached menu images. Recorded only
//! after upload, before mipmaps; ordinary frames sample the filtered image.

use wgpu::util::DeviceExt;

use crate::background::BlurParams;

pub(crate) struct Gaussian {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Gaussian {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-cached-gaussian"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });
        let layout = crate::gpu::uniform_texture_layout(device, "ddi-cached-gaussian-layout");
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-cached-gaussian-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = crate::gpu::fullscreen_pipeline(
            device,
            "ddi-cached-gaussian",
            &pipeline_layout,
            &shader,
            "fs_cached_blur",
            wgpu::TextureFormat::Rgba8Unorm,
            None,
        );
        Self {
            pipeline,
            layout,
            // Clamp edges, without a dark border.
            sampler: crate::gpu::linear_sampler(device, "ddi-cached-gaussian-sampler"),
        }
    }

    pub(crate) fn record(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) {
        let scratch = crate::gpu::texture_2d(
            device,
            "ddi-cached-gaussian-scratch",
            [texture.width(), texture.height()],
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            &[],
        );
        let original = texture.create_view(&wgpu::TextureViewDescriptor {
            mip_level_count: Some(1),
            ..Default::default()
        });
        let temporary = scratch.create_view(&Default::default());
        let steps = [
            [1.0 / texture.width() as f32, 0.0],
            [0.0, 1.0 / texture.height() as f32],
        ];
        for (i, (input, target)) in [(&original, &temporary), (&temporary, &original)]
            .into_iter()
            .enumerate()
        {
            // Only step and the final unpremultiply flag are used by the
            // cached-image entry point.
            let params = BlurParams {
                step: steps[i],
                brightness: 0.0,
                linear: 0.0,
                band: [0.0; 2],
                band_keep: 0.0,
                unpremultiply: i as f32,
                uv_scale: [1.0; 2],
                uv_offset: [0.0; 2],
                opacity: 1.0,
                base: 0.0,
                _pad2: [0.0; 2],
            };
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ddi-cached-gaussian-params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let group = crate::gpu::uniform_texture_group(
                device,
                "ddi-cached-gaussian-group",
                &self.layout,
                buffer.as_entire_binding(),
                input,
                &self.sampler,
            );
            let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
            let mut pass =
                crate::gpu::color_pass(encoder, "ddi-cached-gaussian", target, clear, None);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
