//! Instanced textured quads and triangles, for noteskins: one draw per run
//! with the same texture, blend and wrapping.

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

/// [`TexQuad::texture`] of a shape drawn in its colour alone (a model's
/// mesh without a texture).
pub const UNTEXTURED: u32 = u32::MAX;

/// What a [`TexQuad`] covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Quad {
        /// The unit square (-0.5..0.5) to screen pixels: `x' = m0 x + m2 y
        /// + m4`, `y' = m1 x + m3 y + m5`.
        transform: [f32; 6],
        /// `[left, top, right, bottom]` in texture fractions (may run past
        /// 1 with `wrap`).
        uv: [f32; 4],
    },
    /// A triangle of a model, seen in perspective: its corners in screen
    /// pixels, each corner's `w` (its distance from the camera over the
    /// screen's), textured in perspective.
    Triangle {
        points: [[f32; 2]; 3],
        w: [f32; 3],
        uv: [[f32; 2]; 3],
    },
}

/// One quad or triangle to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexQuad {
    pub shape: Shape,
    /// As the target expects it (see [`crate::color::to_target`]).
    pub color: [f32; 4],
    /// The glow pass rather than the diffuse one.
    pub glow: bool,
    /// A registered texture, or [`UNTEXTURED`].
    pub texture: u32,
    pub blend: QuadBlend,
    /// Repeat the texture (hold bodies) instead of clamping it.
    pub wrap: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Instance {
    /// A quad's linear part and origin; a triangle's corners.
    axes: [f32; 4],
    origin: [f32; 2],
    /// A quad's rectangle; a triangle's first two corners'.
    uv: [f32; 4],
    color: [f32; 4],
    mode: u32,
    /// A triangle's third corner's texture coordinates.
    uv2: [f32; 2],
    /// A triangle's corners' `w` (a quad's are 1).
    w: [f32; 3],
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x4, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Uint32,
        5 => Float32x2, 6 => Float32x3
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
    /// Through the texture's plain view (its texels as encoded), for the
    /// passes on the target's plain view: clamped, repeating.
    plain: Option<[wgpu::BindGroup; 2]>,
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
    /// Normal, add, subtract, modulate blending.
    pipelines: [wgpu::RenderPipeline; 4],
    /// The same to the target's plain view (StepMania's blending).
    gamma: Option<[wgpu::RenderPipeline; 4]>,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    clamp: wgpu::Sampler,
    repeat: wgpu::Sampler,
    textures: Vec<Texture>,
    /// Bound for [`UNTEXTURED`] shapes (its texels unread).
    blank: Option<Texture>,
    instances: wgpu::Buffer,
    capacity: usize,
    batches: Vec<Batch>,
}

impl TexturedPipeline {
    /// Pipelines drawing to `format`, and, with `gamma`, to that plain
    /// (non-sRGB) view of the same target, blending as StepMania does
    /// ([`TexturedPipeline::draw`]).
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        gamma: Option<wgpu::TextureFormat>,
    ) -> TexturedPipeline {
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
        let pipeline = |blend: wgpu::BlendState, format: wgpu::TextureFormat, entry: &str| {
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
                    entry_point: Some(entry),
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
        // `BLEND_NORMAL`, `BLEND_ADD` (source times its alpha, added),
        // `BLEND_SUBTRACT`, `BLEND_MODULATE`.
        let blends = [
            wgpu::BlendState::ALPHA_BLENDING,
            wgpu::BlendState {
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
            },
            wgpu::BlendState {
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
            },
            wgpu::BlendState {
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
            },
        ];
        let pipelines = blends.map(|b| pipeline(b, format, "fs_main"));
        // Drawn to the plain view from the textures' plain views, so texels
        // are filtered and blended as encoded, as StepMania does.
        let gamma = gamma.map(|plain| blends.map(|b| pipeline(b, plain, "fs_main")));
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
        let mut p = TexturedPipeline {
            pipelines,
            gamma,
            globals,
            globals_group,
            texture_layout,
            clamp: sampler(wgpu::AddressMode::ClampToEdge),
            repeat: sampler(wgpu::AddressMode::Repeat),
            textures: Vec::new(),
            blank: None,
            instances: Self::make_buffer(device, capacity),
            capacity,
            batches: Vec::new(),
        };
        let blank = p.create_texture(device, 1, 1, false);
        p.blank = Some(p.bind(device, &blank));
        p
    }

    fn make_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-textured-instances"),
            size: (capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// A texture for [`TexturedPipeline::add_texture`], `width × height`,
    /// sRGB on an sRGB target, with a plain view too when quads are also
    /// drawn to the target's plain view.
    pub fn create_texture(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        srgb: bool,
    ) -> wgpu::Texture {
        let plain = [wgpu::TextureFormat::Rgba8Unorm];
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ddi-textured-image"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: if srgb && self.gamma.is_some() {
                &plain
            } else {
                &[]
            },
        })
    }

    /// Registers a filled texture (from [`TexturedPipeline::create_texture`]);
    /// returns the id quads draw it by.
    pub fn add_texture(&mut self, device: &wgpu::Device, texture: &wgpu::Texture) -> u32 {
        let t = self.bind(device, texture);
        self.textures.push(t);
        self.textures.len() as u32 - 1
    }

    fn bind(&self, device: &wgpu::Device, texture: &wgpu::Texture) -> Texture {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let plain = (self.gamma.is_some()).then(|| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                ..Default::default()
            })
        });
        let group = |view: &wgpu::TextureView, sampler: &wgpu::Sampler| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ddi-textured-texture-bg"),
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        };
        Texture {
            clamp: group(&view, &self.clamp),
            repeat: group(&view, &self.repeat),
            plain: plain.map(|p| [group(&p, &self.clamp), group(&p, &self.repeat)]),
        }
    }

    /// Forgets every texture (a skin was unloaded).
    pub fn clear_textures(&mut self) {
        self.textures.clear();
    }

    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Uploads this frame's quads, in drawing order (as many as one buffer
    /// of the device holds; the rest are not drawn).
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
        let most = (device.limits().max_buffer_size / std::mem::size_of::<Instance>() as u64)
            .min(usize::MAX as u64) as usize;
        let quads = &quads[..quads.len().min(most)];
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
            // Bit 0: the glow pass; bit 1: a repeating texture, sampled
            // past its rectangle; bit 2: a triangle; bit 3: no texture.
            let mode = u32::from(q.glow)
                | (u32::from(q.wrap) << 1)
                | (u32::from(q.texture == UNTEXTURED) << 3);
            instances.push(match q.shape {
                Shape::Quad { transform: m, uv } => Instance {
                    axes: [m[0], m[1], m[2], m[3]],
                    origin: [m[4], m[5]],
                    uv,
                    color: q.color,
                    mode,
                    uv2: [0.0; 2],
                    w: [1.0; 3],
                },
                Shape::Triangle { points: p, w, uv } => Instance {
                    axes: [p[0][0], p[0][1], p[1][0], p[1][1]],
                    origin: p[2],
                    uv: [uv[0][0], uv[0][1], uv[1][0], uv[1][1]],
                    color: q.color,
                    mode: mode | 4,
                    uv2: uv[2],
                    w,
                },
            });
        }
        if instances.len() > self.capacity {
            self.capacity = instances.len().next_power_of_two().min(most);
            self.instances = Self::make_buffer(device, self.capacity);
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
    }

    /// Whether it can draw to a plain view of the target ([`TexturedPipeline::new`]).
    pub fn gamma(&self) -> bool {
        self.gamma.is_some()
    }

    /// Draws the prepared quads `range` (indices into the slice given to
    /// [`TexturedPipeline::prepare`]); quads of an unknown texture are
    /// skipped. `gamma`: in a pass on the plain view.
    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        range: std::ops::Range<u32>,
        gamma: bool,
    ) {
        let pipelines = match (&self.gamma, gamma) {
            (Some(g), true) => g,
            _ => &self.pipelines,
        };
        if range.is_empty() {
            return;
        }
        pass.set_bind_group(0, &self.globals_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        let mut current: Option<QuadBlend> = None;
        for b in &self.batches {
            let (start, end) = (b.start.max(range.start), b.end.min(range.end));
            let t = if b.texture == UNTEXTURED {
                self.blank.as_ref()
            } else {
                self.textures.get(b.texture as usize)
            };
            let Some(t) = t else {
                continue;
            };
            if start >= end {
                continue;
            }
            if current != Some(b.blend) {
                pass.set_pipeline(match b.blend {
                    QuadBlend::Normal => &pipelines[0],
                    QuadBlend::Add => &pipelines[1],
                    QuadBlend::Subtract => &pipelines[2],
                    QuadBlend::Modulate => &pipelines[3],
                });
                current = Some(b.blend);
            }
            let groups = match (&t.plain, gamma) {
                (Some([clamp, repeat]), true) => [clamp, repeat],
                _ => [&t.clamp, &t.repeat],
            };
            pass.set_bind_group(1, groups[usize::from(b.wrap)], &[]);
            pass.draw(0..6, start..end);
        }
    }
}

const _: () = assert!(std::mem::size_of::<Instance>() == 80);

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
