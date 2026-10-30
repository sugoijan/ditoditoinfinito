//! Instanced textured quads and triangles, for noteskins: one draw per run
//! with the same texture, blend, wrapping, mask and material.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use crate::material::{self, MaterialFrame, MaterialParams, MaterialQuad};
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

/// [`TexQuad::texture`] of a shape drawn in its color alone (a model's
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
    /// A color a corner (upper left, upper right, lower left, lower
    /// right; a triangle's first three), as the target expects them,
    /// interpolated over the shape; `None`: `color` alone.
    pub gradient: Option<[[f32; 4]; 4]>,
    pub mask: Mask,
    /// Painted by a material over the texture's alpha (a quad's shape
    /// only), rather than the texture's colors.
    pub material: Option<MaterialQuad>,
}

impl TexQuad {
    /// `shape` in `color` and `texture`: its diffuse pass, blended
    /// normally, the texture clamped, no gradient or mask.
    pub fn new(shape: Shape, color: [f32; 4], texture: u32) -> TexQuad {
        TexQuad {
            shape,
            color,
            glow: false,
            texture,
            blend: QuadBlend::Normal,
            wrap: false,
            gradient: None,
            mask: Mask::None,
            material: None,
        }
    }
}

/// An actor's quad as drawn: its registered texture (`ids` by the ids
/// actors carry; untextured stays so) and its shape, seen in perspective
/// or its transform, its texture rectangle moved by its offset and
/// `offset`. `None` while its texture is not loaded.
pub(crate) fn actor_quad(
    q: &ddi_actor::Quad,
    ids: &[Option<u32>],
    offset: [f32; 2],
) -> Option<(Shape, u32)> {
    let texture = if q.texture == ddi_actor::UNTEXTURED {
        UNTEXTURED
    } else {
        (*ids.get(q.texture as usize)?)?
    };
    let uv = crate::actors2d::offset_uv(
        q.uv,
        [
            offset[0] + q.texcoord_offset[0],
            offset[1] + q.texcoord_offset[1],
        ],
    );
    let shape = match q.projected {
        Some(p) => Shape::Projected {
            corners: p.corners,
            w: p.w,
            uv,
        },
        None => Shape::Quad {
            transform: q.transform,
            uv,
        },
    };
    Some((shape, texture))
}

/// An actor's diffuse and glow passes (`Sprite::DrawTexture`) as `base`
/// (its shape, texture, blend and mask): the diffuse pass when a corner
/// shows (a gradient when the corners' colors differ), the glow's when it
/// shows; colors as the target expects them (`srgb`, see
/// [`crate::color::to_target`]).
pub(crate) fn push_passes(
    base: TexQuad,
    diffuse: [[f32; 4]; 4],
    glow: [f32; 4],
    srgb: bool,
    out: &mut Vec<TexQuad>,
) {
    let colors = diffuse.map(|c| crate::color::to_target(c, srgb));
    let base = TexQuad {
        color: colors[0],
        gradient: colors.iter().any(|c| *c != colors[0]).then_some(colors),
        ..base
    };
    if diffuse.iter().any(|c| c[3] > 0.0) {
        out.push(base);
    }
    if glow[3] > 0.0 {
        out.push(TexQuad {
            color: crate::color::to_target(glow, srgb),
            glow: true,
            gradient: None,
            ..base
        });
    }
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
    /// The corners' colors with a gradient (the first is `color`).
    c1: [f32; 4],
    c2: [f32; 4],
    c3: [f32; 4],
    /// A material's quad: the unit square to material coordinates; its
    /// origin, phase and seed; its value.
    material_axes: [f32; 4],
    material_origin: [f32; 4],
    material_value: [f32; 4],
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 14] = wgpu::vertex_attr_array![
        0 => Float32x4, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Uint32,
        5 => Float32x2, 6 => Float32x4, 7 => Float32x2, 8 => Float32x4, 9 => Float32x4,
        10 => Float32x4, 11 => Float32x4, 12 => Float32x4, 13 => Float32x4
    ];
}

/// Instance mode bits (`textured.wgsl`).
const GLOW: u32 = 1;
const WRAP: u32 = 2;
const TRIANGLE: u32 = 4;
const BLANK: u32 = 8;
const PROJECTED: u32 = 32;
const GRADIENT: u32 = 64;
/// A material's parameters' slot, in the mode's bits 8 to 15.
const SLOT_SHIFT: u32 = 8;

/// `material.wgsl`'s uniforms: the clocks and the target's size, then the
/// parameters' slots.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct MaterialHeader {
    clock: [f32; 4],
    size: [f32; 4],
}

const MATERIAL_UNIFORMS: usize =
    std::mem::size_of::<MaterialHeader>() + material::SLOTS * std::mem::size_of::<MaterialParams>();

/// A material's shader and the pipelines made from it.
struct MaterialShader {
    module: wgpu::ShaderModule,
    pipelines: Pipelines,
    gamma: Option<Pipelines>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Globals {
    scale: [f32; 2],
    translate: [f32; 2],
}

#[derive(Clone)]
struct Texture {
    /// Its views (the plain one with a plain view's passes), for the
    /// materials' images.
    view: wgpu::TextureView,
    plain_view: Option<wgpu::TextureView>,
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
    material: Option<u32>,
    /// A material's images.
    images: [u32; 2],
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

/// What the last frame prepared drew ([`TexturedPipeline::stats`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuadStats {
    /// Instances uploaded.
    pub quads: u32,
    /// Runs of one texture, blend, wrapping and mask: a draw each (one
    /// more each past a buffer's worth of instances).
    pub runs: u32,
    /// Quads clearing the mask.
    pub stencil_clears: u32,
    /// Pipelines made while preparing frames, since this was made: each
    /// a stall in the frame that first needs it.
    pub pipelines_made: u32,
    /// Instances painted by a material.
    pub material_quads: u32,
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
    globals_layout: wgpu::BindGroupLayout,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    clamp: wgpu::Sampler,
    repeat: wgpu::Sampler,
    textures: Vec<Texture>,
    mipmaps: crate::mipmaps::Mipmaps,
    /// Bound for [`UNTEXTURED`] shapes (its texels unread).
    blank: Option<Texture>,
    instances: crate::instances::InstanceBuffers,
    /// This frame's instances before the upload, its room reused.
    staging: Vec<Instance>,
    batches: Vec<Batch>,
    /// Textures quads are drawn into ([`TexturedPipeline::add_target`]):
    /// a song's scripts' `ActorFrameTexture`s.
    targets: Vec<Target>,
    /// The pipelines drawing into them, with their alpha and without
    /// (an `ActorFrameTexture` without an alpha buffer stays opaque).
    target_pipelines: [Pipelines; 2],
    stats: QuadStats,
    /// Materials, by [`MaterialQuad::material`], their uniforms, and the
    /// layout their pipelines share.
    materials: Vec<MaterialShader>,
    material_layout: wgpu::PipelineLayout,
    material_buffer: wgpu::Buffer,
    material_group: wgpu::BindGroup,
    /// The materials' images' bind groups, by images and plain view, and
    /// their layout.
    image_layout: wgpu::BindGroupLayout,
    image_groups: HashMap<([u32; 2], bool), wgpu::BindGroup>,
    /// The skins' materials registered, by their shader's source's hash
    /// (the source, and the id).
    by_source: HashMap<u64, (String, u32)>,
    /// Textures packed into an atlas page ([`TexturedPipeline::pack`]):
    /// by texture id, the page's id and the rectangle there (texture
    /// fractions); the pages, by format.
    atlas: Vec<Option<(u32, [f32; 4])>>,
    pages: Vec<Page>,
    /// The textures registered, kept to be copied into a page.
    sources: Vec<Option<wgpu::Texture>>,
    /// This frame's materials' uniforms, for the next upload.
    material_uniforms: Vec<u8>,
}

/// A texture quads are drawn into, with what draws it.
struct Target {
    /// Its id among the textures.
    id: u32,
    /// The view drawn to: the plain one where quads blend encoded.
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    instances: crate::instances::InstanceBuffers,
    staging: Vec<Instance>,
    batches: Vec<Batch>,
    /// Quads given for the next frame: with an alpha buffer, kept from
    /// the last frame (`EnablePreserveTexture`).
    pending: Option<(bool, bool)>,
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
        let (globals, globals_layout, globals_group) = crate::gpu::uniform_group(
            device,
            "ddi-textured-globals",
            std::mem::size_of::<Globals>(),
            wgpu::ShaderStages::VERTEX,
        );
        let texture_layout = crate::gpu::texture_sampler_layout(device, "ddi-textured-texture-bgl");
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-textured-layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let (material_buffer, material_bind_layout, material_group) = crate::gpu::uniform_group(
            device,
            "ddi-textured-materials",
            MATERIAL_UNIFORMS,
            wgpu::ShaderStages::FRAGMENT,
        );
        let image = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-textured-material-images-bgl"),
            entries: &[
                image(0),
                image(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let material_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-textured-material-layout"),
            bind_group_layouts: &[
                Some(&globals_layout),
                Some(&texture_layout),
                Some(&material_bind_layout),
                Some(&image_layout),
            ],
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
            globals_layout,
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
            staging: Vec::new(),
            batches: Vec::new(),
            targets: Vec::new(),
            target_pipelines: [Pipelines::new(), Pipelines::new()],
            stats: QuadStats::default(),
            materials: Vec::new(),
            material_layout,
            material_buffer,
            material_group,
            image_layout,
            image_groups: HashMap::new(),
            by_source: HashMap::new(),
            atlas: Vec::new(),
            pages: Vec::new(),
            sources: Vec::new(),
            material_uniforms: Vec::new(),
        };
        let blank = p.create_texture(device, 1, 1, false, false);
        p.blank = Some(p.bind(device, &blank));
        p
    }

    /// Registers a material ([`material::compile`]), its pipelines made
    /// now for `blends`, with and without the mask (any other is made when
    /// a frame first needs it): the id its quads name.
    pub fn add_material(
        &mut self,
        device: &wgpu::Device,
        compiled: &material::Compiled,
        blends: &[QuadBlend],
    ) -> u32 {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-textured-material"),
            source: wgpu::ShaderSource::Wgsl(compiled.source.as_str().into()),
        });
        let id = self.materials.len();
        self.materials.push(MaterialShader {
            module,
            pipelines: Pipelines::new(),
            gamma: self.gamma.as_ref().map(|_| Pipelines::new()),
        });
        for &blend in blends {
            for kind in [None, Some(Kind::Level), Some(Kind::Masked)] {
                for gamma in [false, true] {
                    self.ensure_pipeline(device, Some(id as u32), (blend, kind), gamma);
                }
            }
        }
        id as u32
    }

    /// A skin's material ([`material::compile`]) registered once for its
    /// source, its pipelines made for normal and additive blending: its id.
    pub fn skin_material(&mut self, device: &wgpu::Device, compiled: &material::Compiled) -> u32 {
        if let Some((source, id)) = self.by_source.get(&compiled.key)
            && *source == compiled.source
        {
            return *id;
        }
        let id = self.add_material(device, compiled, &[QuadBlend::Normal, QuadBlend::Add]);
        self.by_source
            .insert(compiled.key, (compiled.source.clone(), id));
        id
    }

    /// A note layer's own materials (`skins`) registered, and its quads
    /// naming them ([`material::LOCAL`]) given their ids; a quad naming
    /// none is drawn without a material.
    pub fn name_skin_materials(
        &mut self,
        device: &wgpu::Device,
        skins: &[material::Compiled],
        quads: &mut [TexQuad],
    ) {
        let ids: Vec<u32> = skins
            .iter()
            .map(|c| self.skin_material(device, c))
            .collect();
        for q in quads {
            if let Some(m) = &mut q.material
                && m.material & material::LOCAL != 0
            {
                match ids.get((m.material & !material::LOCAL) as usize) {
                    Some(&id) => m.material = id,
                    None => q.material = None,
                }
            }
        }
    }

    /// The materials' inputs of the next frame prepared.
    pub fn set_materials(&mut self, frame: MaterialFrame<'_>) {
        // Images are sampled in linear light through an sRGB texture's own
        // view: where quads blend in linear light on an sRGB target.
        let images_linear = self.gamma.is_none() && self.format.is_srgb();
        let header = MaterialHeader {
            clock: [
                frame.beat,
                frame.seconds,
                if frame.linear { 1.0 } else { 0.0 },
                if images_linear { 1.0 } else { 0.0 },
            ],
            size: [0.0; 4],
        };
        self.material_uniforms.clear();
        self.material_uniforms
            .extend_from_slice(bytemuck::bytes_of(&header));
        let params = &frame.params[..frame.params.len().min(material::SLOTS)];
        self.material_uniforms
            .extend_from_slice(bytemuck::cast_slice(params));
    }

    /// The pipelines drawing through `material` (none: without one), to
    /// a gamma target or not; `None` where there are none of those.
    fn pipeline_set(&self, material: Option<u32>, gamma: bool) -> Option<&Pipelines> {
        let (plain, with_gamma) = match material {
            None => (&self.pipelines, &self.gamma),
            Some(m) => {
                let shader = &self.materials[m as usize];
                (&shader.pipelines, &shader.gamma)
            }
        };
        if gamma {
            with_gamma.as_ref()
        } else {
            Some(plain)
        }
    }

    /// [`TexturedPipeline::pipeline_set`], to add to.
    fn pipeline_set_mut(&mut self, material: Option<u32>, gamma: bool) -> Option<&mut Pipelines> {
        let (plain, with_gamma) = match material {
            None => (&mut self.pipelines, &mut self.gamma),
            Some(m) => {
                let shader = &mut self.materials[m as usize];
                (&mut shader.pipelines, &mut shader.gamma)
            }
        };
        if gamma {
            with_gamma.as_mut()
        } else {
            Some(plain)
        }
    }

    /// Makes the pipeline of `key` for `material` (or the textured one)
    /// when it is not made yet; whether it was.
    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        material: Option<u32>,
        key: (QuadBlend, Option<Kind>),
        gamma: bool,
    ) -> bool {
        if self
            .pipeline_set(material, gamma)
            .is_none_or(|p| p.get(key).is_some())
        {
            return false;
        }
        let format = match (gamma, self.gamma_format) {
            (true, Some(f)) => f,
            _ => self.format,
        };
        let p = self.make_pipeline_for(
            device,
            material,
            format,
            blend_state(key.0),
            key.1.map(stencil::state),
            wgpu::ColorWrites::ALL,
        );
        if let Some(set) = self.pipeline_set_mut(material, gamma) {
            set.insert(key, p);
        }
        true
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
        crate::gpu::texture_2d(
            device,
            "ddi-textured-image",
            [width.max(1), height.max(1)],
            if srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            if mipmaps {
                crate::mipmaps::levels(width, height)
            } else {
                1
            },
            wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            if srgb && self.gamma.is_some() {
                &plain
            } else {
                &[]
            },
        )
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
        self.sources.push(Some(texture.clone()));
        self.textures.len() as u32 - 1
    }

    /// Packs textures `ids` (a skin's images drawn whole or by frame,
    /// never repeated: `NoteLayer::atlas_textures`) into atlas pages, each
    /// once, by GPU copies recorded on `encoder`; quads drawing one then
    /// draw from its page (their texture coordinates moved there), so a
    /// skin's parts batch into a few draws instead of one each. A texture
    /// too large for a page, with mipmaps or of another use stays alone.
    pub fn pack(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, ids: &[u32]) {
        /// The space kept between textures in a page.
        const GAP: u32 = 2;
        let side = device.limits().max_texture_dimension_2d.min(4096);
        self.atlas.resize(self.textures.len(), None);
        let mut todo: Vec<(u32, wgpu::Texture)> = ids
            .iter()
            .filter(|&&id| self.atlas.get(id as usize).is_some_and(Option::is_none))
            .filter(|&&id| !self.targets.iter().any(|t| t.id == id))
            .filter_map(|&id| {
                let t = self.sources.get(id as usize)?.clone()?;
                let fits = t.width() + GAP <= side / 2 && t.height() + GAP <= side / 2;
                (fits && t.mip_level_count() == 1).then_some((id, t))
            })
            .collect();
        if todo.is_empty() {
            return;
        }
        // Tallest first, onto shelves.
        todo.sort_by_key(|(id, t)| (std::cmp::Reverse(t.height()), *id));
        for k in 0..todo.len() {
            let (id, source) = &todo[k];
            let (id, source) = (*id, source.clone());
            let (w, h) = (source.width() + GAP, source.height() + GAP);
            let format = source.format();
            let place = self.pages.iter_mut().find_map(|p| {
                (p.format == format)
                    .then(|| p.place(w, h))
                    .flatten()
                    .map(|at| (p.id, at, p.side))
            });
            let (page, at, page_side) = match place {
                Some(found) => found,
                None => {
                    // As large as what is left to place in this format needs.
                    let left = todo[k..].iter().filter(|(_, t)| t.format() == format);
                    let (area, tallest) = left.fold((0u64, 0u32), |(a, m), (_, t)| {
                        let (tw, th) = (t.width() + GAP, t.height() + GAP);
                        (a + u64::from(tw) * u64::from(th), m.max(tw).max(th))
                    });
                    let side = page_side(area, tallest, side);
                    let texture = self.create_texture(device, side, side, format.is_srgb(), false);
                    let page_id = self.add_texture(device, &texture);
                    self.atlas.resize(self.textures.len(), None);
                    let mut page = Page {
                        id: page_id,
                        format: texture.format(),
                        side,
                        packer: crate::atlas::ShelfPacker::new(side, side).with_waste(1.0),
                    };
                    let at = page.place(w, h).expect("an empty page holds it");
                    self.pages.push(page);
                    (page_id, at, side)
                }
            };
            let Some(Some(target)) = self.sources.get(page as usize) else {
                continue;
            };
            encoder.copy_texture_to_texture(
                source.as_image_copy(),
                wgpu::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: at[0],
                        y: at[1],
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                source.size(),
            );
            let s = page_side as f32;
            self.atlas[id as usize] = Some((
                page,
                [
                    at[0] as f32 / s,
                    at[1] as f32 / s,
                    (at[0] + source.width()) as f32 / s,
                    (at[1] + source.height()) as f32 / s,
                ],
            ));
            // The texture lives on in its page alone.
            self.sources[id as usize] = None;
            if let Some(page) = self.textures.get(page as usize).cloned() {
                self.textures[id as usize] = page;
            }
        }
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
            crate::gpu::texture_sampler_group(
                device,
                "ddi-textured-texture-bg",
                &self.texture_layout,
                view,
                sampler,
            )
        };
        Texture {
            clamp: group(&view, &self.clamp),
            repeat: group(&view, &self.repeat),
            plain: plain
                .as_ref()
                .map(|p| [group(p, &self.clamp), group(p, &self.repeat)]),
            view,
            plain_view: plain,
        }
    }

    /// Forgets every texture (a skin was unloaded), and the targets.
    pub fn clear_textures(&mut self) {
        self.textures.clear();
        self.targets.clear();
        self.image_groups.clear();
        self.atlas.clear();
        self.pages.clear();
        self.sources.clear();
    }

    /// The bind group of a material's images (through their plain views
    /// with `plain`), made when first needed.
    fn ensure_images(&mut self, device: &wgpu::Device, images: [u32; 2], plain: bool) {
        if self.image_groups.contains_key(&(images, plain)) {
            return;
        }
        let view = |id: u32| {
            let t = if id == UNTEXTURED {
                self.blank.as_ref()
            } else {
                self.textures.get(id as usize).or(self.blank.as_ref())
            }?;
            Some(match (&t.plain_view, plain) {
                (Some(p), true) => p,
                _ => &t.view,
            })
        };
        let (Some(a), Some(b)) = (view(images[0]), view(images[1])) else {
            return;
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-textured-material-images"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(a),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(b),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.repeat),
                },
            ],
        });
        self.image_groups.insert((images, plain), group);
    }

    /// A texture of `width × height` quads are drawn into
    /// ([`TexturedPipeline::set_target_quads`]), registered as a filled
    /// one is: the id quads draw it by. sRGB as [`TexturedPipeline::create_texture`]
    /// makes it; drawn through its plain view where quads blend encoded.
    pub fn add_target(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        srgb: bool,
    ) -> u32 {
        let texture = self.create_texture(device, width, height, srgb, false);
        let id = self.add_texture(device, &texture);
        let format = if self.gamma.is_some() {
            texture.format().remove_srgb_suffix()
        } else {
            texture.format()
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            ..Default::default()
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-textured-target-globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-textured-target-globals-bg"),
            layout: &self.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        self.targets.push(Target {
            id,
            view,
            format,
            size: [width.max(1), height.max(1)],
            globals,
            globals_group,
            instances: crate::instances::InstanceBuffers::new(
                device,
                "ddi-textured-target-instances",
                std::mem::size_of::<Instance>(),
            ),
            staging: Vec::new(),
            batches: Vec::new(),
            pending: None,
        });
        id
    }

    /// The quads target `id` ([`TexturedPipeline::add_target`]) is drawn
    /// with in the next frame ([`TexturedPipeline::draw_targets`]), in its
    /// pixels; with `alpha` its alpha is kept (else it is opaque, black
    /// where nothing is drawn), with `preserve` what it held is drawn
    /// over (else it is cleared). Quads of the target's own texture, and
    /// those using the mask, are left out.
    pub fn set_target_quads(&mut self, id: u32, quads: &[TexQuad], alpha: bool, preserve: bool) {
        let Some(t) = self.targets.iter_mut().find(|t| t.id == id) else {
            return;
        };
        let quads: Vec<TexQuad> = quads
            .iter()
            .filter(|q| q.texture != id && q.mask == Mask::None && q.material.is_none())
            .copied()
            .collect();
        build(&quads, &self.atlas, &mut t.staging, &mut t.batches);
        t.pending = Some((alpha, preserve));
    }

    /// Draws the targets given quads since the last frame, each in a pass
    /// of its own, before the frame's (their textures are then sampled
    /// as drawn this frame).
    pub fn draw_targets(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        for i in 0..self.targets.len() {
            let Some((alpha, preserve)) = self.targets[i].pending.take() else {
                continue;
            };
            // The pipelines its runs need.
            let format = self.targets[i].format;
            for b in 0..self.targets[i].batches.len() {
                let blend = self.targets[i].batches[b].blend;
                if self.target_pipelines[usize::from(alpha)]
                    .get((blend, None))
                    .is_none()
                {
                    let write = if alpha {
                        wgpu::ColorWrites::ALL
                    } else {
                        wgpu::ColorWrites::COLOR
                    };
                    let p = self.make_pipeline_for(
                        device,
                        None,
                        format,
                        blend_state(blend),
                        None,
                        write,
                    );
                    self.target_pipelines[usize::from(alpha)].insert((blend, None), p);
                }
            }
            let t = &mut self.targets[i];
            let g = Globals {
                scale: [2.0 / t.size[0] as f32, -2.0 / t.size[1] as f32],
                translate: [-1.0, 1.0],
            };
            queue.write_buffer(&t.globals, 0, bytemuck::bytes_of(&g));
            t.instances
                .upload(device, queue, bytemuck::cast_slice(&t.staging));
            let t = &self.targets[i];
            let load = if preserve {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(if alpha {
                    wgpu::Color::TRANSPARENT
                } else {
                    wgpu::Color::BLACK
                })
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ddi-textured-target"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &t.globals_group, &[]);
            let pipelines = &self.target_pipelines[usize::from(alpha)];
            let gamma = self.gamma.is_some();
            let mut current = None;
            for b in &t.batches {
                let tex = if b.texture == UNTEXTURED {
                    self.blank.as_ref()
                } else {
                    self.textures.get(b.texture as usize)
                };
                let Some(tex) = tex else {
                    continue;
                };
                if current != Some(b.blend) {
                    let Some(p) = pipelines.get((b.blend, None)) else {
                        continue;
                    };
                    pass.set_pipeline(p);
                    current = Some(b.blend);
                }
                let groups = match (&tex.plain, gamma) {
                    (Some([clamp, repeat]), true) => [clamp, repeat],
                    _ => [&tex.clamp, &tex.repeat],
                };
                pass.set_bind_group(1, groups[usize::from(b.wrap)], &[]);
                for (buffer, range) in t.instances.pieces(b.start..b.end) {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, range);
                }
            }
        }
    }

    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// The pipeline drawing `blend` to `format`, writing `write`, with
    /// `material`'s shader or the textured one.
    fn make_pipeline_for(
        &self,
        device: &wgpu::Device,
        material: Option<u32>,
        format: wgpu::TextureFormat,
        blend: wgpu::BlendState,
        depth_stencil: Option<wgpu::DepthStencilState>,
        write: wgpu::ColorWrites,
    ) -> wgpu::RenderPipeline {
        let (module, layout, fragment) = match material {
            Some(m) => (
                &self.materials[m as usize].module,
                &self.material_layout,
                "fs_material",
            ),
            None => (&self.shader, &self.layout, "fs_main"),
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ddi-textured-pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &Instance::ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fragment),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: write,
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
        let mut instances = std::mem::take(&mut self.staging);
        build(quads, &self.atlas, &mut instances, &mut self.batches);
        // The pipelines this frame's runs need: in passes with the stencil
        // too once a quad uses the mask.
        self.masked = quads.iter().any(|q| q.mask != Mask::None);
        for i in 0..self.batches.len() {
            let b = self.batches[i];
            if b.material
                .is_some_and(|m| m as usize >= self.materials.len())
            {
                continue;
            }
            // Outside a pass with the stencil only what tests no mask;
            // inside one, with the mask's state, once a quad uses the mask.
            let plain = (b.mask == Mask::None).then_some((b.blend, None));
            let stenciled = self.masked.then(|| (b.blend, Some(b.mask.stencil(0).0)));
            for key in [plain, stenciled].into_iter().flatten() {
                for gamma in [false, true] {
                    if self.ensure_pipeline(device, b.material, key, gamma) {
                        self.stats.pipelines_made += 1;
                    }
                }
            }
        }
        let plains: &[bool] = if self.gamma.is_some() {
            &[false, true]
        } else {
            &[false]
        };
        for i in 0..self.batches.len() {
            let b = self.batches[i];
            if b.material.is_some() {
                for &plain in plains {
                    self.ensure_images(device, b.images, plain);
                }
            }
        }
        if self.batches.iter().any(|b| b.material.is_some()) {
            // The target's size, then the uniforms as they were set.
            if self.material_uniforms.len() < std::mem::size_of::<MaterialHeader>() {
                self.set_materials(MaterialFrame::default());
            }
            let size = [width, height, 0.0, 0.0];
            self.material_uniforms[16..32].copy_from_slice(bytemuck::cast_slice(&size));
            queue.write_buffer(&self.material_buffer, 0, &self.material_uniforms);
        }
        self.instances
            .upload(device, queue, bytemuck::cast_slice(&instances));
        self.stats.quads = instances.len() as u32;
        self.stats.runs = self.batches.len() as u32;
        self.stats.stencil_clears = quads.iter().filter(|q| q.mask == Mask::Clear).count() as u32;
        self.stats.material_quads = quads.iter().filter(|q| q.material.is_some()).count() as u32;
        self.staging = instances;
    }

    /// What the last frame prepared drew.
    pub fn stats(&self) -> QuadStats {
        self.stats
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
        let mut current: Option<(Option<u32>, QuadBlend, Option<Kind>)> = None;
        let mut materials_bound = false;
        let mut images_bound: Option<[u32; 2]> = None;
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
            if current != Some((b.material, key.0, key.1)) {
                let set = match b.material {
                    None => Some(pipelines),
                    Some(m) => self
                        .materials
                        .get(m as usize)
                        .map(|s| match (&s.gamma, gamma) {
                            (Some(g), true) => g,
                            _ => &s.pipelines,
                        }),
                };
                let Some(p) = set.and_then(|s| s.get(key)) else {
                    continue;
                };
                pass.set_pipeline(p);
                current = Some((b.material, key.0, key.1));
                if b.material.is_some() && !materials_bound {
                    pass.set_bind_group(2, &self.material_group, &[]);
                    materials_bound = true;
                }
            }
            if b.material.is_some() && images_bound != Some(b.images) {
                let Some(group) = self.image_groups.get(&(b.images, gamma)) else {
                    continue;
                };
                pass.set_bind_group(3, group, &[]);
                images_bound = Some(b.images);
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

/// `quads`' instances, in order, and their runs of one texture, blend,
/// wrapping and mask.
fn build(
    quads: &[TexQuad],
    atlas: &[Option<(u32, [f32; 4])>],
    instances: &mut Vec<Instance>,
    batches: &mut Vec<Batch>,
) {
    batches.clear();
    instances.clear();
    instances.reserve(quads.len());
    for q in quads {
        let at = instances.len() as u32;
        // A packed texture is drawn from its page, its coordinates moved
        // into its rectangle there.
        let (texture, page) = match atlas.get(q.texture as usize).copied().flatten() {
            Some((page, rect)) if !q.wrap => (page, Some(rect)),
            _ => (q.texture, None),
        };
        let moved = |[u, v]: [f32; 2]| match page {
            Some(r) => [r[0] + (r[2] - r[0]) * u, r[1] + (r[3] - r[1]) * v],
            None => [u, v],
        };
        let moved4 = |uv: [f32; 4]| {
            let [a, b] = moved([uv[0], uv[1]]);
            let [c, d] = moved([uv[2], uv[3]]);
            [a, b, c, d]
        };
        match batches.last_mut() {
            Some(b)
                if b.texture == texture
                    && b.blend == q.blend
                    && b.wrap == q.wrap
                    && b.mask == q.mask
                    && b.material == q.material.map(|m| m.material)
                    && b.images == q.material.map_or([UNTEXTURED; 2], |m| m.images) =>
            {
                b.end += 1
            }
            _ => batches.push(Batch {
                texture,
                blend: q.blend,
                wrap: q.wrap,
                mask: q.mask,
                material: q.material.map(|m| m.material),
                images: q.material.map_or([UNTEXTURED; 2], |m| m.images),
                start: at,
                end: at + 1,
            }),
        }
        let mode = if q.glow { GLOW } else { 0 }
            | if q.wrap { WRAP } else { 0 }
            | if q.texture == UNTEXTURED { BLANK } else { 0 }
            | if q.gradient.is_some() { GRADIENT } else { 0 }
            | q.material
                .map_or(0, |m| u32::from(m.slot.min(255)) << SLOT_SHIFT);
        let [c0, c1, c2, c3] = q.gradient.unwrap_or([q.color; 4]);
        let m = q.material.unwrap_or(MaterialQuad {
            material: 0,
            slot: 0,
            axes: [0.0; 4],
            origin: [0.0; 2],
            phase: 0.0,
            seed: 0.0,
            value: [0.0; 4],
            images: [UNTEXTURED; 2],
        });
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
            material_axes: m.axes,
            material_origin: [m.origin[0], m.origin[1], m.phase, m.seed],
            material_value: m.value,
        };
        instances.push(match q.shape {
            Shape::Quad { transform: m, uv } => Instance {
                axes: [m[0], m[1], m[2], m[3]],
                origin: [m[4], m[5]],
                uv: moved4(uv),
                ..base
            },
            Shape::Triangle { points: p, w, uv } => Instance {
                axes: [p[0][0], p[0][1], p[1][0], p[1][1]],
                origin: p[2],
                uv: moved4([uv[0][0], uv[0][1], uv[1][0], uv[1][1]]),
                mode: mode | TRIANGLE,
                uv2: moved(uv[2]),
                w: [w[0], w[1], w[2], 1.0],
                ..base
            },
            Shape::Projected { corners: p, w, uv } => Instance {
                axes: [p[0][0], p[0][1], p[1][0], p[1][1]],
                origin: p[2],
                corner: p[3],
                uv: moved4(uv),
                mode: mode | PROJECTED,
                w,
                ..base
            },
        });
    }
}

const _: () = assert!(std::mem::size_of::<Instance>() == 188);

/// An atlas page's side for textures of `area` pixels in all, the largest
/// `largest` pixels on a side: a power of two holding them as shelves
/// usually do (about two thirds full), from 256 to `most`.
fn page_side(area: u64, largest: u32, most: u32) -> u32 {
    let wanted = ((area as f64 * 1.5).sqrt().ceil() as u32)
        .max(largest)
        .max(256);
    wanted.checked_next_power_of_two().unwrap_or(most).min(most)
}

/// An atlas page ([`TexturedPipeline::pack`]): its texture's id, format
/// and side, and its shelves (the images placed tallest first, so any
/// shorter one goes on a shelf with room).
struct Page {
    id: u32,
    format: wgpu::TextureFormat,
    side: u32,
    packer: crate::atlas::ShelfPacker,
}

impl Page {
    /// A place for `w × h` pixels; `None` when the page is full for it.
    fn place(&mut self, w: u32, h: u32) -> Option<[u32; 2]> {
        self.packer.alloc(w, h).map(|r| [r.x, r.y])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn atlas_pages_are_as_large_as_their_textures_need() {
        // A few small parts: a small page.
        assert_eq!(super::page_side(12 * 130 * 130, 130, 4096), 1024);
        assert_eq!(super::page_side(4 * 66 * 66, 66, 4096), 256);
        // A skin's worth: the largest page the device allows.
        assert_eq!(super::page_side(20_000_000, 400, 4096), 4096);
        assert_eq!(super::page_side(20_000_000, 400, 2048), 2048);
    }

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
