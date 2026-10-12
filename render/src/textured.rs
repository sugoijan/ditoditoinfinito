//! Instanced textured quads, for noteskins: one draw per run of quads with
//! the same texture, blend and wrapping.

use bytemuck::{Pod, Zeroable};

/// How a quad blends (`BlendMode` Normal and Add; the rest draw as normal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuadBlend {
    Normal,
    Add,
    /// `BLEND_SUBTRACT`: the source times its alpha taken from what is
    /// there.
    Subtract,
    /// `BLEND_MODULATE`: what is there times the source.
    Modulate,
}

/// One quad to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexQuad {
    /// The unit square (-0.5..0.5) to screen pixels: `x' = m0 x + m2 y + m4`,
    /// `y' = m1 x + m3 y + m5`.
    pub transform: [f32; 6],
    /// `[left, top, right, bottom]` in texture fractions (may run past 1
    /// with `wrap`).
    pub uv: [f32; 4],
    /// As the target expects it (see [`crate::color::to_target`]).
    pub color: [f32; 4],
    /// The glow pass rather than the diffuse one.
    pub glow: bool,
    pub texture: u32,
    pub blend: QuadBlend,
    /// Repeat the texture (hold bodies) instead of clamping it.
    pub wrap: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Instance {
    axes: [f32; 4],
    origin: [f32; 2],
    uv: [f32; 4],
    color: [f32; 4],
    mode: u32,
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x4, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Uint32
    ];
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Globals {
    scale: [f32; 2],
    translate: [f32; 2],
}

struct Texture {
    clamp: wgpu::BindGroup,
    repeat: wgpu::BindGroup,
}

#[derive(Clone, Copy, Debug)]
struct Batch {
    texture: u32,
    blend: QuadBlend,
    wrap: bool,
    start: u32,
    end: u32,
}

pub struct TexturedPipeline {
    normal: wgpu::RenderPipeline,
    add: wgpu::RenderPipeline,
    subtract: wgpu::RenderPipeline,
    modulate: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    clamp: wgpu::Sampler,
    repeat: wgpu::Sampler,
    textures: Vec<Texture>,
    instances: wgpu::Buffer,
    capacity: usize,
    batches: Vec<Batch>,
}

impl TexturedPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> TexturedPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-textured"),
            source: wgpu::ShaderSource::Wgsl(include_str!("textured.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-textured-globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-textured-globals-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-textured-globals-bg"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-textured-texture-bgl"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-textured-layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ddi-textured-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &Instance::ATTRIBUTES,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
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
            })
        };
        let normal = pipeline(wgpu::BlendState::ALPHA_BLENDING);
        // `BLEND_ADD`: source times its alpha, added.
        let add = pipeline(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });
        let subtract = pipeline(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::ReverseSubtract,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });
        let modulate = pipeline(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });
        let sampler = |mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ddi-textured-sampler"),
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        let capacity = 256;
        TexturedPipeline {
            normal,
            add,
            subtract,
            modulate,
            globals,
            globals_group,
            texture_layout,
            clamp: sampler(wgpu::AddressMode::ClampToEdge),
            repeat: sampler(wgpu::AddressMode::Repeat),
            textures: Vec::new(),
            instances: Self::make_buffer(device, capacity),
            capacity,
            batches: Vec::new(),
        }
    }

    fn make_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-textured-instances"),
            size: (capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Registers a filled texture; returns the id quads draw it by.
    pub fn add_texture(&mut self, device: &wgpu::Device, texture: &wgpu::Texture) -> u32 {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let group = |sampler: &wgpu::Sampler| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ddi-textured-texture-bg"),
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        };
        self.textures.push(Texture {
            clamp: group(&self.clamp),
            repeat: group(&self.repeat),
        });
        self.textures.len() as u32 - 1
    }

    /// Forgets every texture (a skin was unloaded).
    pub fn clear_textures(&mut self) {
        self.textures.clear();
    }

    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Uploads this frame's quads, in drawing order.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: f32,
        height: f32,
        quads: &[TexQuad],
    ) {
        let g = Globals {
            scale: [2.0 / width.max(1.0), -2.0 / height.max(1.0)],
            translate: [-1.0, 1.0],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
        self.batches.clear();
        let mut instances = Vec::with_capacity(quads.len());
        for q in quads {
            let at = instances.len() as u32;
            match self.batches.last_mut() {
                Some(b) if b.texture == q.texture && b.blend == q.blend && b.wrap == q.wrap => {
                    b.end += 1
                }
                _ => self.batches.push(Batch {
                    texture: q.texture,
                    blend: q.blend,
                    wrap: q.wrap,
                    start: at,
                    end: at + 1,
                }),
            }
            let m = q.transform;
            instances.push(Instance {
                axes: [m[0], m[1], m[2], m[3]],
                origin: [m[4], m[5]],
                uv: q.uv,
                color: q.color,
                // Bit 0: the glow pass; bit 1: a repeating texture, sampled
                // past its rectangle.
                mode: u32::from(q.glow) | (u32::from(q.wrap) << 1),
            });
        }
        if instances.len() > self.capacity {
            self.capacity = instances.len().next_power_of_two();
            self.instances = Self::make_buffer(device, self.capacity);
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
    }

    /// Draws the prepared quads `range` (indices into the slice given to
    /// [`TexturedPipeline::prepare`]); quads of an unknown texture are
    /// skipped.
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, range: std::ops::Range<u32>) {
        if range.is_empty() {
            return;
        }
        pass.set_bind_group(0, &self.globals_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        let mut current: Option<QuadBlend> = None;
        for b in &self.batches {
            let (start, end) = (b.start.max(range.start), b.end.min(range.end));
            let Some(t) = self.textures.get(b.texture as usize) else {
                continue;
            };
            if start >= end {
                continue;
            }
            if current != Some(b.blend) {
                pass.set_pipeline(match b.blend {
                    QuadBlend::Normal => &self.normal,
                    QuadBlend::Add => &self.add,
                    QuadBlend::Subtract => &self.subtract,
                    QuadBlend::Modulate => &self.modulate,
                });
                current = Some(b.blend);
            }
            pass.set_bind_group(1, if b.wrap { &t.repeat } else { &t.clamp }, &[]);
            pass.draw(0..6, start..end);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shader_validates() {
        let source = include_str!("textured.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("WGSL validates");
    }
}
