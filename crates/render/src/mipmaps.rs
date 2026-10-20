//! Mipmaps for a texture already filled at level 0, drawn level by level
//! (WebGPU and WebGL2 make none themselves): for noteskins' model textures,
//! which StepMania draws mipmapped (`docs/plans/models.md`), and the 2D
//! pass's images (`crate::painter`), which are drawn smaller than decoded.

use std::collections::HashMap;

pub(crate) struct Mipmaps {
    shader: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    /// By the format the levels are drawn in.
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

/// The levels a `width × height` texture has, down to 1 × 1.
pub(crate) fn levels(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

impl Mipmaps {
    pub(crate) fn new(device: &wgpu::Device) -> Mipmaps {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-mipmaps"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mipmaps.wgsl").into()),
        });
        let layout = crate::gpu::texture_sampler_layout(device, "ddi-mipmaps-bgl");
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-mipmaps-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Mipmaps {
            shader,
            layout,
            pipeline_layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ddi-mipmaps-sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            pipelines: HashMap::new(),
        }
    }

    /// Fills `texture`'s levels below the first, each drawn from the one
    /// above it through views in `format` (the texture's own, or one of
    /// its view formats).
    pub(crate) fn generate(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        format: wgpu::TextureFormat,
    ) {
        if texture.mip_level_count() < 2 {
            return;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ddi-mipmaps"),
        });
        self.record(device, &mut encoder, texture, format);
        queue.submit(Some(encoder.finish()));
    }

    /// [`Mipmaps::generate`] into `encoder`, after whatever filled the
    /// first level (a queue's write or copy, done before what is
    /// submitted next).
    pub(crate) fn record(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        format: wgpu::TextureFormat,
    ) {
        if texture.mip_level_count() < 2 {
            return;
        }
        let pipeline = self.pipelines.entry(format).or_insert_with(|| {
            crate::gpu::fullscreen_pipeline(
                device,
                "ddi-mipmaps-pipeline",
                &self.pipeline_layout,
                &self.shader,
                "fs_main",
                format,
                None,
            )
        });
        let view = |level| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("ddi-mipmaps-level"),
                format: Some(format),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        for level in 1..texture.mip_level_count() {
            let (source, target) = (view(level - 1), view(level));
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ddi-mipmaps-bg"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ddi-mipmaps"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_go_down_to_one_texel() {
        assert_eq!(levels(1, 1), 1);
        assert_eq!(levels(256, 256), 9);
        assert_eq!(levels(300, 64), 9);
        assert_eq!(levels(0, 0), 1);
    }

    #[test]
    fn shader_validates() {
        let source = include_str!("mipmaps.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("WGSL validates");
    }
}
