// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Bounded-resolution Euclidean silhouette dilation, composited behind the scene.
//! No readback, compute shaders, float render targets or changes to geometry.
use ddi_character::appearance::Appearance;
use wgpu::util::DeviceExt;
struct Targets {
    output: wgpu::Texture,
    view: wgpu::TextureView,
    seed: wgpu::TextureView,
    distance: wgpu::TextureView,
    coverage: wgpu::TextureView,
    size: [u32; 2],
    mask_size: [u32; 2],
}
pub(super) struct Backing {
    layout: wgpu::BindGroupLayout,
    pipelines: [wgpu::RenderPipeline; 4],
    sampler: wgpu::Sampler,
    settings: wgpu::Buffer,
    targets: Option<Targets>,
}
impl Backing {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("character-backing"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("character-backing"),
            source: wgpu::ShaderSource::Wgsl(include_str!("character_backing.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("character-backing"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipelines = ["fs_seed", "fs_horizontal", "fs_vertical", "fs_finish"].map(|entry| {
            crate::gpu::fullscreen_pipeline(
                device,
                entry,
                &pl,
                &shader,
                entry,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            )
        });
        Self {
            layout,
            pipelines,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("character-backing"),
                min_filter: wgpu::FilterMode::Linear,
                mag_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            settings: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("character-backing-settings"),
                contents: bytemuck::cast_slice(&[0_f32; 8]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            targets: None,
        }
    }
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().map(|t| &t.output)
    }
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        size: [u32; 2],
        look: Appearance,
    ) {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let scale = (512. / size[0].max(size[1]) as f32).min(1.);
            let mask_size = size.map(|v| (v as f32 * scale).round().max(1.) as u32);
            let make = |size: [u32; 2]| {
                crate::gpu::texture_2d(
                    device,
                    "character-backing",
                    size,
                    wgpu::TextureFormat::Rgba8Unorm,
                    1,
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    &[],
                )
            };
            let output = make(size);
            let view = output.create_view(&Default::default());
            self.targets = Some(Targets {
                output,
                view,
                seed: make(mask_size).create_view(&Default::default()),
                distance: make(mask_size).create_view(&Default::default()),
                coverage: make(mask_size).create_view(&Default::default()),
                size,
                mask_size,
            });
        }
        let t = self.targets.as_ref().unwrap();
        let color = look.backing_color.map(|v| v.clamp(0., 1.));
        let radius = (look.backing_margin.clamp(0., 0.06)
            * t.mask_size[0].min(t.mask_size[1]) as f32)
            .min(30.);
        queue.write_buffer(
            &self.settings,
            0,
            bytemuck::cast_slice(&[
                color[0],
                color[1],
                color[2],
                look.backing_opacity.clamp(0., 1.),
                radius,
                t.mask_size[0] as f32,
                t.mask_size[1] as f32,
                0.,
            ]),
        );
        for (i, (input, output)) in [
            (source, &t.seed),
            (&t.seed, &t.distance),
            (&t.distance, &t.coverage),
            (&t.coverage, &t.view),
        ]
        .into_iter()
        .enumerate()
        {
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("character-backing"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.settings.as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("character-backing"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
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
            pass.set_pipeline(&self.pipelines[i]);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn backing_shader_validates_and_translates_for_webgl2() {
        let module = naga::front::wgsl::parse_str(include_str!("character_backing.wgsl")).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        for (stage, entry) in [
            (naga::ShaderStage::Vertex, "vs_main"),
            (naga::ShaderStage::Fragment, "fs_seed"),
            (naga::ShaderStage::Fragment, "fs_horizontal"),
            (naga::ShaderStage::Fragment, "fs_vertical"),
            (naga::ShaderStage::Fragment, "fs_finish"),
        ] {
            let mut output = String::new();
            let options = naga::back::glsl::Options {
                version: naga::back::glsl::Version::Embedded {
                    version: 300,
                    is_webgl: true,
                },
                ..Default::default()
            };
            let pipeline = naga::back::glsl::PipelineOptions {
                shader_stage: stage,
                entry_point: entry.into(),
                multiview: None,
            };
            naga::back::glsl::Writer::new(
                &mut output,
                &module,
                &info,
                &options,
                &pipeline,
                Default::default(),
            )
            .unwrap()
            .write()
            .unwrap();
        }
    }
}
