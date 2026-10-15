//! Instanced textured quads and triangles, for noteskins: one draw per run
//! with the same texture, blend, wrapping and mask.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use crate::stencil::{self, Kind, Mask};

/// How a quad blends: StepMania's `BlendMode`s
/// (`RageDisplay_Legacy::SetBlendMode`, `src/RageDisplay_OGL.cpp:1869-1934`),
/// the target's alpha kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuadBlend {
    Normal,
    Add,
    /// `BLEND_SUBTRACT`: the source times its alpha taken from what is
    /// there times one less the source's alpha.
    Subtract,
    /// `BLEND_MODULATE`: what is there times the source.
    Modulate,
    /// `BLEND_COPY_SRC`: the source replaces what is there.
    CopySrc,
    /// `BLEND_ALPHA_MASK` and `BLEND_ALPHA_KNOCK_OUT`: they change only
    /// the target's alpha, which stays as it is here (on WebGL2 the
    /// canvas's alpha shows the page through), so they draw nothing.
    AlphaMask,
    AlphaKnockOut,
    /// `BLEND_ALPHA_MULTIPLY`: the source times its alpha, nothing of
    /// what is there.
    AlphaMultiply,
    /// `BLEND_WEIGHTED_MULTIPLY`: twice what is there times the source
    /// (grey leaves it as it is).
    WeightedMultiply,
    /// `BLEND_INVERT_DEST`: the source less what is there.
    InvertDest,
    /// `BLEND_NO_EFFECT`: nothing drawn (a mask's source).
    NoEffect,
}

/// [`TexQuad::texture`] of a shape drawn in its colour alone (a model's
/// mesh without a texture).
pub const UNTEXTURED: u32 = u32::MAX;

/// What a [`TexQuad`] covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A quad seen in perspective (under an `fov` frame): its corners on
    /// the screen (upper left, upper right, lower left, lower right) with
    /// their `w`, textured in perspective.
    Projected {
        corners: [[f32; 2]; 4],
        w: [f32; 4],
        uv: [f32; 4],
    },
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
    /// A colour a corner (upper left, upper right, lower left, lower
    /// right; a triangle's first three), as the target expects them,
    /// interpolated over the shape; `None`: `color` alone.
    pub gradient: Option<[[f32; 4]; 4]>,
    pub mask: Mask,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Instance {
    /// A quad's linear part and origin; a triangle's corners; a projected
    /// quad's upper corners and its lower left.
    axes: [f32; 4],
    origin: [f32; 2],
    /// A quad's rectangle; a triangle's first two corners'.
    uv: [f32; 4],
    color: [f32; 4],
    mode: u32,
    /// A triangle's third corner's texture coordinates.
    uv2: [f32; 2],
    /// The corners' `w` (a quad's are 1).
    w: [f32; 4],
    /// A projected quad's lower right corner.
    corner: [f32; 2],
    /// The corners' colours with a gradient (the first is `color`).
    c1: [f32; 4],
    c2: [f32; 4],
    c3: [f32; 4],
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 11] = wgpu::vertex_attr_array![
        0 => Float32x4, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Uint32,
        5 => Float32x2, 6 => Float32x4, 7 => Float32x2, 8 => Float32x4, 9 => Float32x4,
        10 => Float32x4
    ];
}

/// Instance mode bits (`textured.wgsl`).
const GLOW: u32 = 1;
const WRAP: u32 = 2;
const TRIANGLE: u32 = 4;
const BLANK: u32 = 8;
const PROJECTED: u32 = 32;
const GRADIENT: u32 = 64;

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
    mask: Mask,
    start: u32,
    end: u32,
}

/// The blend state of `blend`, for straight alpha, as
/// `RageDisplay_Legacy::SetBlendMode` sets it
/// (`src/RageDisplay_OGL.cpp:1875-1926`). The target's alpha is kept but
/// for normal blending: on WebGL2 it would let the page show through the
/// canvas.
pub(crate) fn blend_state(blend: QuadBlend) -> wgpu::BlendState {
    use wgpu::BlendFactor::{Dst, One, OneMinusSrcAlpha, Src, SrcAlpha, Zero};
    use wgpu::BlendOperation::{Add, ReverseSubtract, Subtract};
    let c = |src_factor, dst_factor, operation| wgpu::BlendComponent {
        src_factor,
        dst_factor,
        operation,
    };
    let keep = c(Zero, One, Add);
    let (color, alpha) = match blend {
        QuadBlend::Normal => return wgpu::BlendState::ALPHA_BLENDING,
        QuadBlend::Add => (c(SrcAlpha, One, Add), keep),
        // `GL_FUNC_REVERSE_SUBTRACT` with `GL_SRC_ALPHA`,
        // `GL_ONE_MINUS_SRC_ALPHA` (lines 1877-1878, 1893-1895): what is
        // there times one less the source's alpha, less the source times
        // its alpha.
        QuadBlend::Subtract => (c(SrcAlpha, OneMinusSrcAlpha, ReverseSubtract), keep),
        QuadBlend::Modulate => (c(Zero, Src, Add), keep),
        QuadBlend::CopySrc => (c(One, Zero, Add), keep),
        QuadBlend::AlphaMask | QuadBlend::AlphaKnockOut => (keep, keep),
        QuadBlend::AlphaMultiply => (c(SrcAlpha, Zero, Add), keep),
        QuadBlend::WeightedMultiply => (c(Dst, Src, Add), keep),
        QuadBlend::InvertDest => (c(One, One, Subtract), keep),
        QuadBlend::NoEffect => (keep, keep),
    };
    wgpu::BlendState { color, alpha }
}

/// Pipelines made as they are first needed: by blend mode and stencil
/// state (`None`: a pass with no stencil).
pub(crate) struct Pipelines {
    made: HashMap<(QuadBlend, Option<Kind>), wgpu::RenderPipeline>,
}

impl Pipelines {
    pub(crate) fn new() -> Pipelines {
        Pipelines {
            made: HashMap::new(),
        }
    }

    pub(crate) fn insert(
        &mut self,
        key: (QuadBlend, Option<Kind>),
        pipeline: wgpu::RenderPipeline,
    ) {
        self.made.insert(key, pipeline);
    }

    pub(crate) fn get(&self, key: (QuadBlend, Option<Kind>)) -> Option<&wgpu::RenderPipeline> {
        self.made.get(&key)
    }
}

pub struct TexturedPipeline {
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    pipelines: Pipelines,
    /// The plain view's format (StepMania's blending), and its pipelines.
    gamma_format: Option<wgpu::TextureFormat>,
    gamma: Option<Pipelines>,
    /// Some of this frame's quads use the mask: their runs are drawn in
    /// passes with the stencil ([`TexturedPipeline::masked`]).
    masked: bool,
    /// Textures are made plain (their texels as encoded, sampled so),
    /// not sRGB with a plain view: a target with no other view of its
    /// textures (WebGL2) draws quads only to a plain texture.
    plain_textures: bool,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    clamp: wgpu::Sampler,
    repeat: wgpu::Sampler,
    textures: Vec<Texture>,
    mipmaps: crate::mipmaps::Mipmaps,
    /// Bound for [`UNTEXTURED`] shapes (its texels unread).
    blank: Option<Texture>,
    instances: crate::instances::InstanceBuffers,
    batches: Vec<Batch>,
}

impl TexturedPipeline {
    /// Pipelines drawing to `format`, and, with `gamma`, to that plain
    /// (non-sRGB) view of the same target, blending as StepMania does
    /// ([`TexturedPipeline::draw`]); with `plain_textures`, quads are only
    /// drawn there and their textures are plain.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        gamma: Option<wgpu::TextureFormat>,
        plain_textures: bool,
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
        let plain_textures = plain_textures && gamma.is_some();
        let sampler = |mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ddi-textured-sampler"),
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                // Between a model texture's mipmaps (the others have none).
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        let mut p = TexturedPipeline {
            shader,
            layout,
            format,
            pipelines: Pipelines::new(),
            gamma_format: gamma,
            gamma: gamma.map(|_| Pipelines::new()),
            masked: false,
            plain_textures,
            globals,
            globals_group,
            texture_layout,
            clamp: sampler(wgpu::AddressMode::ClampToEdge),
            repeat: sampler(wgpu::AddressMode::Repeat),
            textures: Vec::new(),
            mipmaps: crate::mipmaps::Mipmaps::new(device),
            blank: None,
            instances: crate::instances::InstanceBuffers::new(
                device,
                "ddi-textured-instances",
                std::mem::size_of::<Instance>(),
            ),
            batches: Vec::new(),
        };
        let blank = p.create_texture(device, 1, 1, false, false);
        p.blank = Some(p.bind(device, &blank));
        p
    }

    /// A texture for [`TexturedPipeline::add_texture`], `width × height`,
    /// sRGB on an sRGB target, with a plain view too when quads are also
    /// drawn to the target's plain view; plain with `plain_textures`. With
    /// `mipmaps`, every level down to one texel, for
    /// [`TexturedPipeline::generate_mipmaps`] once the first is filled.
    pub fn create_texture(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        srgb: bool,
        mipmaps: bool,
    ) -> wgpu::Texture {
        let plain = [wgpu::TextureFormat::Rgba8Unorm];
        let srgb = srgb && !self.plain_textures;
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ddi-textured-image"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: if mipmaps {
                crate::mipmaps::levels(width, height)
            } else {
                1
            },
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

    /// Fills the mipmaps of a texture from [`TexturedPipeline::create_texture`]
    /// whose first level is filled: averaged as the skin's quads blend
    /// (through the plain view, encoded values, as StepMania's 8-bit
    /// mipmaps are, when quads are drawn there).
    pub fn generate_mipmaps(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
    ) {
        let format = if self.gamma.is_some() {
            texture.format().remove_srgb_suffix()
        } else {
            texture.format()
        };
        self.mipmaps.generate(device, queue, texture, format);
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
                format: Some(texture.format().remove_srgb_suffix()),
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

    /// The pipeline drawing `blend` with the stencil state of `kind` to
    /// the target (`gamma`: its plain view).
    fn make_pipeline(
        &self,
        device: &wgpu::Device,
        gamma: bool,
        blend: wgpu::BlendState,
        depth_stencil: Option<wgpu::DepthStencilState>,
    ) -> wgpu::RenderPipeline {
        let format = match (gamma, self.gamma_format) {
            (true, Some(f)) => f,
            _ => self.format,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ddi-textured-pipeline"),
            layout: Some(&self.layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &Instance::ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
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
            depth_stencil,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Whether some of the prepared quads use the mask: the runs holding
    /// them are drawn in passes with the stencil.
    pub fn masked(&self) -> bool {
        self.masked
    }

    /// Whether quads `range` use the mask.
    pub fn masked_in(&self, range: std::ops::Range<u32>) -> bool {
        self.batches
            .iter()
            .any(|b| b.mask != Mask::None && b.start < range.end && b.end > range.start)
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
                Some(b)
                    if b.texture == q.texture
                        && b.blend == q.blend
                        && b.wrap == q.wrap
                        && b.mask == q.mask =>
                {
                    b.end += 1
                }
                _ => self.batches.push(Batch {
                    texture: q.texture,
                    blend: q.blend,
                    wrap: q.wrap,
                    mask: q.mask,
                    start: at,
                    end: at + 1,
                }),
            }
            let mode = if q.glow { GLOW } else { 0 }
                | if q.wrap { WRAP } else { 0 }
                | if q.texture == UNTEXTURED { BLANK } else { 0 }
                | if q.gradient.is_some() { GRADIENT } else { 0 };
            let [c0, c1, c2, c3] = q.gradient.unwrap_or([q.color; 4]);
            let base = Instance {
                axes: [0.0; 4],
                origin: [0.0; 2],
                uv: [0.0; 4],
                color: c0,
                mode,
                uv2: [0.0; 2],
                w: [1.0; 4],
                corner: [0.0; 2],
                c1,
                c2,
                c3,
            };
            instances.push(match q.shape {
                Shape::Quad { transform: m, uv } => Instance {
                    axes: [m[0], m[1], m[2], m[3]],
                    origin: [m[4], m[5]],
                    uv,
                    ..base
                },
                Shape::Triangle { points: p, w, uv } => Instance {
                    axes: [p[0][0], p[0][1], p[1][0], p[1][1]],
                    origin: p[2],
                    uv: [uv[0][0], uv[0][1], uv[1][0], uv[1][1]],
                    mode: mode | TRIANGLE,
                    uv2: uv[2],
                    w: [w[0], w[1], w[2], 1.0],
                    ..base
                },
                Shape::Projected { corners: p, w, uv } => Instance {
                    axes: [p[0][0], p[0][1], p[1][0], p[1][1]],
                    origin: p[2],
                    corner: p[3],
                    uv,
                    mode: mode | PROJECTED,
                    w,
                    ..base
                },
            });
        }
        // The pipelines this frame's runs need: in passes with the stencil
        // too once a quad uses the mask.
        self.masked = quads.iter().any(|q| q.mask != Mask::None);
        for i in 0..self.batches.len() {
            let (blend, mask) = (self.batches[i].blend, self.batches[i].mask);
            // Outside a pass with the stencil only what tests no mask;
            // inside one, with the mask's state, once a quad uses the mask.
            let plain = (mask == Mask::None).then_some((blend, None));
            let stenciled = self.masked.then(|| (blend, Some(mask.stencil(0).0)));
            for key in [plain, stenciled].into_iter().flatten() {
                for gamma in [false, true] {
                    let set = if gamma {
                        self.gamma.as_ref()
                    } else {
                        Some(&self.pipelines)
                    };
                    if set.is_none_or(|p| p.get(key).is_some()) {
                        continue;
                    }
                    let p = self.make_pipeline(
                        device,
                        gamma,
                        blend_state(key.0),
                        key.1.map(stencil::state),
                    );
                    match (gamma, self.gamma.as_mut()) {
                        (true, Some(g)) => g.insert(key, p),
                        _ => self.pipelines.insert(key, p),
                    }
                }
            }
        }
        self.instances
            .upload(device, queue, bytemuck::cast_slice(&instances));
    }

    /// Whether it can draw to a plain view of the target ([`TexturedPipeline::new`]).
    pub fn gamma(&self) -> bool {
        self.gamma.is_some()
    }

    /// Draws the prepared quads `range` (indices into the slice given to
    /// [`TexturedPipeline::prepare`]); quads of an unknown texture are
    /// skipped. `gamma`: in a pass on the plain view; `stencil`: in a pass
    /// with the stencil, which those using the mask need (they are
    /// skipped elsewhere).
    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        range: std::ops::Range<u32>,
        gamma: bool,
        stencil: bool,
    ) {
        let pipelines = match (&self.gamma, gamma) {
            (Some(g), true) => g,
            _ => &self.pipelines,
        };
        if range.is_empty() {
            return;
        }
        pass.set_bind_group(0, &self.globals_group, &[]);
        let mut current: Option<(QuadBlend, Option<Kind>)> = None;
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
            let (kind, reference) = b.mask.stencil(0);
            let key = (b.blend, stencil.then_some(kind));
            if current != Some(key) {
                let Some(p) = pipelines.get(key) else {
                    continue;
                };
                pass.set_pipeline(p);
                current = Some(key);
            }
            if stencil {
                pass.set_stencil_reference(reference);
            }
            let groups = match (&t.plain, gamma) {
                (Some([clamp, repeat]), true) => [clamp, repeat],
                _ => [&t.clamp, &t.repeat],
            };
            pass.set_bind_group(1, groups[usize::from(b.wrap)], &[]);
            // Past one buffer's worth, a run is drawn from each buffer.
            for (buffer, range) in self.instances.pieces(start..end) {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..6, range);
            }
        }
    }
}

const _: () = assert!(std::mem::size_of::<Instance>() == 140);

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

    #[test]
    fn stepmanias_blend_modes() {
        use super::{QuadBlend, blend_state};
        use wgpu::BlendFactor as F;
        // Nothing of no effect, nor of the modes about alpha alone.
        for b in [
            QuadBlend::NoEffect,
            QuadBlend::AlphaMask,
            QuadBlend::AlphaKnockOut,
        ] {
            let s = blend_state(b);
            assert_eq!((s.color.src_factor, s.color.dst_factor), (F::Zero, F::One));
        }
        let w = blend_state(QuadBlend::WeightedMultiply);
        assert_eq!((w.color.src_factor, w.color.dst_factor), (F::Dst, F::Src));
        let s = blend_state(QuadBlend::Subtract);
        assert_eq!(
            (s.color.src_factor, s.color.dst_factor, s.color.operation),
            (
                F::SrcAlpha,
                F::OneMinusSrcAlpha,
                wgpu::BlendOperation::ReverseSubtract
            )
        );
        let i = blend_state(QuadBlend::InvertDest);
        assert_eq!(i.color.operation, wgpu::BlendOperation::Subtract);
        // The target's alpha is kept by all but normal blending.
        assert_eq!(blend_state(QuadBlend::CopySrc).alpha.dst_factor, F::One);
    }
}
