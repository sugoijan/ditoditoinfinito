//! The song's background image behind the field.
//!
//! The renderer never decodes images: the shell creates the texture with
//! [`BackgroundPipeline::create_texture`] and fills it (the web shell copies
//! an `ImageBitmap` the browser decoded; a desktop shell would write RGBA
//! bytes), then hands it over with [`BackgroundPipeline::set_texture`].

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Params {
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    brightness: f32,
    _pad: [f32; 3],
}

/// Texture coordinates `(scale, offset)` that cover a `target_w × target_h`
/// area with an `image_w × image_h` image, cropping the overflow evenly.
pub fn cover_uv(target_w: f32, target_h: f32, image_w: f32, image_h: f32) -> ([f32; 2], [f32; 2]) {
    if target_w <= 0.0 || target_h <= 0.0 || image_w <= 0.0 || image_h <= 0.0 {
        return ([1.0, 1.0], [0.0, 0.0]);
    }
    let target = target_w / target_h;
    let image = image_w / image_h;
    if image > target {
        let s = target / image;
        ([s, 1.0], [(1.0 - s) / 2.0, 0.0])
    } else {
        let s = image / target;
        ([1.0, s], [0.0, (1.0 - s) / 2.0])
    }
}

/// Multiplier for a perceived brightness in `0..=1`. On an sRGB target the
/// shader works in linear light, where 0.4 would look much brighter than
/// 40 %; on a plain target the image's values pass through encoded.
pub fn brightness_factor(perceived: f32, srgb_target: bool) -> f32 {
    let p = perceived.clamp(0.0, 1.0);
    if srgb_target { p.powf(2.2) } else { p }
}

pub struct BackgroundPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// Bind group and image size of the current texture.
    current: Option<(wgpu::BindGroup, f32, f32)>,
    /// The target format is sRGB.
    srgb: bool,
}

impl BackgroundPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> BackgroundPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-background"),
            source: wgpu::ShaderSource::Wgsl(include_str!("background.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-background-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-background-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ddi-background-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-background-params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ddi-background-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        BackgroundPipeline {
            pipeline,
            layout,
            params,
            sampler,
            current: None,
            srgb: format.is_srgb(),
        }
    }

    /// An empty texture of the right format and usage for
    /// [`BackgroundPipeline::set_texture`], for sRGB-encoded pixels. On an
    /// sRGB target it is an sRGB texture, so sampling returns linear light
    /// like the target expects; on a plain target the encoded values pass
    /// straight through, like every other colour in the scene.
    /// `RENDER_ATTACHMENT` is required by WebGPU's
    /// `copyExternalImageToTexture`.
    pub fn create_texture(&self, device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ddi-background-image"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if self.srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    /// Shows `texture` (from [`BackgroundPipeline::create_texture`]), or
    /// nothing.
    pub fn set_texture(&mut self, device: &wgpu::Device, texture: Option<&wgpu::Texture>) {
        self.current = texture.map(|t| {
            let view = t.create_view(&wgpu::TextureViewDescriptor::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ddi-background-bg"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let size = t.size();
            (group, size.width as f32, size.height as f32)
        });
    }

    pub fn has_texture(&self) -> bool {
        self.current.is_some()
    }

    /// Uploads the parameters for a `width × height` target; returns
    /// whether there is anything to draw.
    pub fn prepare(&self, queue: &wgpu::Queue, width: f32, height: f32, brightness: f32) -> bool {
        let Some((_, iw, ih)) = &self.current else {
            return false;
        };
        if brightness <= 0.0 {
            return false;
        }
        let (uv_scale, uv_offset) = cover_uv(width, height, *iw, *ih);
        let p = Params {
            uv_scale,
            uv_offset,
            brightness: brightness_factor(brightness, self.srgb),
            _pad: [0.0; 3],
        };
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&p));
        true
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if let Some((group, ..)) = &self.current {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_crops_the_overflowing_side_evenly() {
        // A 4:3 image on a 16:9 target: full width, top and bottom cropped.
        let (s, o) = cover_uv(1600.0, 900.0, 640.0, 480.0);
        assert!((s[0] - 1.0).abs() < 1e-6 && (s[1] - 0.75).abs() < 1e-6);
        assert!((o[1] - 0.125).abs() < 1e-6 && o[0] == 0.0);
        // A wide image on a portrait target: full height, sides cropped.
        let (s, o) = cover_uv(400.0, 800.0, 800.0, 400.0);
        assert!((s[0] - 0.25).abs() < 1e-6 && s[1] == 1.0);
        assert!((o[0] - 0.375).abs() < 1e-6);
        // Same aspect: the whole image.
        assert_eq!(
            cover_uv(640.0, 480.0, 320.0, 240.0),
            ([1.0, 1.0], [0.0, 0.0])
        );
    }

    #[test]
    fn brightness_is_perceptual_on_either_target() {
        for srgb in [false, true] {
            assert_eq!(brightness_factor(0.0, srgb), 0.0);
            assert_eq!(brightness_factor(1.0, srgb), 1.0);
            assert_eq!(brightness_factor(2.0, srgb), 1.0);
        }
        assert!((brightness_factor(0.5, true) - 0.5f32.powf(2.2)).abs() < 1e-6);
        assert_eq!(brightness_factor(0.5, false), 0.5);
    }
}
