//! The 2D pass (`docs/plans/ui-core.md`, P11 and step 2): draws a
//! [`DrawList`] through a plain (non-sRGB) view of the target, blending in
//! encoded values as StepMania and the browser do, with no offscreen pass.
//!
//! It owns the two caches a screen draws from: textures by source (P9,
//! [`crate::texture_cache`]) and text runs on atlas pages (P4,
//! [`crate::text_atlas`], drawn by the platform's [`RunRasterizer`]). Both
//! keep their bookkeeping across a lost device ([`Painter::device_changed`]):
//! texture ids stay, and textures and runs are loaded and drawn again as
//! they are shown.
//!
//! Every quad and run is one instance of one shader; consecutive
//! instances with the same texture, blend mode and clip are one draw, and
//! a frame of any size is drawn ([`crate::instances`]).

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use ddi_actor::text::{FixedMeasure, RunMetrics, TextMeasure, TextStyle};

use crate::atlas::PageRect;
use crate::draw2d::{Blend, ClipStack, Command, DrawList, Mask, scissor};
use crate::instances::InstanceBuffers;
use crate::stencil::{self, Kind};
use crate::text_atlas::{Measurer, PAGE, PageSink, RunCache, RunRasterizer, TextStats};
use crate::texture_cache::{TextureCache, TextureId, TextureKey, TextureStats};
use crate::textured::Pipelines;

/// Bytes of textures kept by default (P9: the platform may set its own).
pub const TEXTURE_BUDGET: u64 = 96 << 20;

/// Bytes of text pages kept by default: eight pages of 2048², enough for
/// a screen of song titles at a device pixel ratio of 2 (step 0 drew
/// 2,000 of them on 13–14 pages; a screen shows a few dozen).
pub const TEXT_BUDGET: u64 = 8 * (PAGE as u64) * (PAGE as u64);

/// Frames a text page past the budget stays unused before it goes.
const PAGE_TRIM_FRAMES: u64 = 120;

/// What a quad whose texture is still loading is drawn in.
const PLACEHOLDER: [f32; 3] = [0.16, 0.16, 0.2];

/// The scale a run is drawn at for `transform`, over the device's: its
/// size on the screen, rounded to a quarter of an octave (a zoom tween
/// draws a run again a few times, not every frame), from 1/4 to 8.
fn run_zoom(transform: &crate::draw2d::Affine) -> f32 {
    let m = transform.0;
    let det = (m[0] * m[3] - m[1] * m[2]).abs();
    if !(det.is_finite() && det > 0.0) {
        return 1.0;
    }
    let steps = (det.sqrt().log2() * 4.0).round().clamp(-8.0, 12.0);
    (steps / 4.0).exp2()
}

/// Instance mode bits (`painter.wgsl`).
const COVERAGE: u32 = 1;
const UNTEXTURED: u32 = 2;
const GLOW: u32 = 4;
const PROJECTED: u32 = 8;
/// Cut by `shape` (rounded corners, a soft edge, an outline).
const CUT: u32 = 16;
/// Its colour's saturation changed (`shape2`'s z).
const SATURATION: u32 = 32;
/// Smeared along `blur` (a motion blur).
const BLUR: u32 = 64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Instance {
    /// The linear part; a projected quad's top corners, in pixels.
    axes: [f32; 4],
    origin: [f32; 2],
    mode: u32,
    _pad: u32,
    /// The rectangle in its own space; a projected quad's bottom right
    /// and bottom left corners, in pixels.
    rect: [f32; 4],
    uv: [f32; 4],
    /// Top left, top right, bottom right, bottom left, as 8-bit RGBA.
    colors: [u32; 4],
    /// The corners' `w`, in the same order (1 but in perspective).
    w: [f32; 4],
    /// With [`CUT`]: the corners' radius and the soft edge, in the
    /// rectangle's units per axis.
    shape: [f32; 4],
    /// The outline's thickness per axis; the saturation.
    shape2: [f32; 4],
    /// With [`BLUR`]: the blur's vector in the rectangle's units.
    blur: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<Instance>() == 144);

impl Instance {
    /// By the fields' offsets (`_pad` keeps `rect` 16-byte aligned).
    const ATTRIBUTES: [wgpu::VertexAttribute; 13] = {
        const fn a(
            shader_location: u32,
            format: wgpu::VertexFormat,
            offset: u64,
        ) -> wgpu::VertexAttribute {
            wgpu::VertexAttribute {
                format,
                offset,
                shader_location,
            }
        }
        use wgpu::VertexFormat::{Float32x2, Float32x4, Uint32, Unorm8x4};
        [
            a(0, Float32x4, 0),
            a(1, Float32x2, 16),
            a(2, Uint32, 24),
            a(3, Float32x4, 32),
            a(4, Float32x4, 48),
            a(5, Unorm8x4, 64),
            a(6, Unorm8x4, 68),
            a(7, Unorm8x4, 72),
            a(8, Unorm8x4, 76),
            a(9, Float32x4, 80),
            a(10, Float32x4, 96),
            a(11, Float32x4, 112),
            a(12, Float32x4, 128),
        ]
    };
}

/// No outline, full saturation.
const NO_SHAPE2: [f32; 4] = [0.0, 0.0, 1.0, 0.0];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Globals {
    scale: [f32; 2],
    translate: [f32; 2],
    /// 1 when the target holds linear light (an sRGB view): colours are
    /// decoded before blending, and the target encodes them again.
    linear: f32,
    _pad: [f32; 3],
}

/// A colour as 8-bit RGBA (memory order r, g, b, a).
fn pack(c: [f32; 4]) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    b(c[0]) | b(c[1]) << 8 | b(c[2]) << 16 | b(c[3]) << 24
}

/// A texture of the cache on the device, with its mipmaps (filled at the
/// next draw).
pub struct Image {
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
}

struct Page {
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
}

/// What a draw samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bind {
    Blank,
    Image(TextureId),
    Page(usize),
    /// An image the shell draws itself ([`Painter::set_external_image`]).
    External(usize),
}

/// One draw: instances `start..end` with the same texture, blend,
/// scissor rectangle and stencil state and reference (`None`: a frame
/// without the stencil).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Batch {
    start: u32,
    end: u32,
    bind: Bind,
    blend: Blend,
    scissor: [u32; 4],
    stencil: Option<(Kind, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Draw(Batch),
    External(u32),
}

/// Adds instance `index` to the last draw when it can join it.
fn add(
    steps: &mut Vec<Step>,
    index: u32,
    bind: Bind,
    blend: Blend,
    scissor: [u32; 4],
    stencil: Option<(Kind, u32)>,
) {
    if let Some(Step::Draw(b)) = steps.last_mut()
        && b.end == index
        && b.bind == bind
        && b.blend == blend
        && b.scissor == scissor
        && b.stencil == stencil
    {
        b.end += 1;
        return;
    }
    steps.push(Step::Draw(Batch {
        start: index,
        end: index + 1,
        bind,
        blend,
        scissor,
        stencil,
    }));
}

/// A clip counted in the stencil: its quad (units) and the scissor it
/// was counted within.
type CountedClip = ([[f32; 2]; 4], [u32; 4]);

/// A draw's stencil state and reference.
type Stencil = (Kind, u32);

/// A clip quad's draw when it is popped: its quad, scissor and stencil.
type ClipOut = ([[f32; 2]; 4], [u32; 4], Stencil);

/// The clips in force while a list is drawn: the scissor of their
/// rectangles, and with the stencil (`stencil`) the level of those that
/// are quads ([`crate::stencil`]).
struct Clips {
    rects: ClipStack,
    /// `None`: nothing is left to draw in.
    scissor: Option<[u32; 4]>,
    /// Each clip pushed: its quad and scissor when it counted a level.
    counted: Vec<Option<CountedClip>>,
    level: u32,
    stencil: bool,
    scale: f32,
    size: [u32; 2],
}

impl Clips {
    fn new(stencil: bool, scale: f32, size: [u32; 2]) -> Clips {
        Clips {
            rects: ClipStack::default(),
            scissor: scissor(None, scale, size),
            counted: Vec::new(),
            level: 0,
            stencil,
            scale,
            size,
        }
    }

    fn push(&mut self, r: crate::draw2d::Rect) {
        self.rects.push(r);
        self.scissor = scissor(self.rects.current(), self.scale, self.size);
        self.counted.push(None);
    }

    /// A clip's quad within `bound`: the scissor and stencil state its
    /// quad is drawn with to count a level where it is (within the clips
    /// around), `None` when it counts none (no stencil, nothing left, or
    /// too many).
    fn push_quad(
        &mut self,
        corners: &[[f32; 2]; 4],
        bound: crate::draw2d::Rect,
    ) -> Option<([u32; 4], Stencil)> {
        let count = self
            .scissor
            .filter(|_| self.stencil && self.level < stencil::MAX_LEVEL);
        let drawn = count.map(|sc| (sc, (Kind::ClipIn, self.level)));
        if count.is_some() {
            self.level += 1;
        }
        self.rects.push(bound);
        self.scissor = scissor(self.rects.current(), self.scale, self.size);
        self.counted.push(count.map(|sc| (*corners, sc)));
        drawn
    }

    /// Pops a clip: a counted one's quad again, its scissor and stencil
    /// state, to take its level back.
    fn pop(&mut self) -> Option<ClipOut> {
        let taken = match self.counted.pop() {
            Some(Some((corners, sc))) => {
                let out = (corners, sc, (Kind::ClipOut, self.level));
                self.level -= 1;
                Some(out)
            }
            _ => None,
        };
        self.rects.pop();
        self.scissor = scissor(self.rects.current(), self.scale, self.size);
        taken
    }

    /// A draw's scissor and stencil state (`None`: not drawn); a mask's
    /// clear covers the whole target, clips or not.
    fn draw(&self, mask: Mask) -> Option<([u32; 4], Option<Stencil>)> {
        let sc = if mask == Mask::Clear {
            [0, 0, self.size[0], self.size[1]]
        } else {
            self.scissor?
        };
        Some((sc, self.stencil.then(|| mask.stencil(self.level))))
    }
}

/// Whether drawing `list` needs the stencil: a mask, or a clip that is
/// no rectangle.
fn uses_stencil(list: &DrawList) -> bool {
    list.commands.iter().any(|c| match c {
        Command::Quad(q) => q.mask != Mask::None,
        Command::Text(t) => t.mask != Mask::None,
        Command::PushClipQuad { .. } => true,
        _ => false,
    })
}

/// A run's cell, `rect` in its line box's space, seen through
/// `perspective`, in pixels (`scale` a unit), with each corner's `w`
/// (top left, top right, bottom right, bottom left); `None` when a corner
/// is outside the camera's depth.
fn project_rect(
    (m, camera): &([f32; 16], ddi_actor::Perspective),
    rect: [f32; 4],
    scale: f32,
) -> Option<([f32; 4], [f32; 4], [f32; 4])> {
    let mut points = [[0.0; 2]; 4];
    let mut w = [0.0; 4];
    for (i, (x, y)) in [
        (rect[0], rect[1]),
        (rect[2], rect[1]),
        (rect[2], rect[3]),
        (rect[0], rect[3]),
    ]
    .into_iter()
    .enumerate()
    {
        let p = ddi_actor::transform_point(m, [x, y, 0.0]);
        let (at, cw) = camera.project(p)?;
        points[i] = [at[0] * scale, at[1] * scale];
        w[i] = cw;
    }
    Some((
        [points[0][0], points[0][1], points[1][0], points[1][1]],
        [points[2][0], points[2][1], points[3][0], points[3][1]],
        w,
    ))
}

/// The device's objects; made again on a new device.
struct Gpu {
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    /// By blend and stencil state, made as first needed.
    pipelines: Pipelines,
    stencil: stencil::Attachment,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    blank: wgpu::BindGroup,
    instances: InstanceBuffers,
    pages: Vec<Option<Page>>,
    max_side: u32,
    mipmaps: crate::mipmaps::Mipmaps,
}

impl Gpu {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Gpu {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-painter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("painter.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-painter-globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-painter-globals-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-painter-globals-bg"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-painter-texture-bgl"),
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
            label: Some("ddi-painter-layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        // Images have mipmaps (drawn smaller than decoded, as a banner
        // kept large is in a row); text pages do not.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ddi-painter-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white = texture(
            device,
            [1, 1],
            wgpu::TextureFormat::Rgba8Unorm,
            "ddi-painter-blank",
            1,
        );
        queue.write_texture(
            white.as_image_copy(),
            &[255; 4],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let blank = bind(device, &texture_layout, &sampler, &white);
        Gpu {
            shader,
            layout,
            format,
            pipelines: Pipelines::new(),
            stencil: Default::default(),
            globals,
            globals_group,
            texture_layout,
            sampler,
            blank,
            instances: InstanceBuffers::new(
                device,
                "ddi-painter-instances",
                std::mem::size_of::<Instance>(),
            ),
            pages: Vec::new(),
            max_side: device.limits().max_texture_dimension_2d,
            mipmaps: crate::mipmaps::Mipmaps::new(device),
        }
    }
}

impl Gpu {
    /// Makes the pipeline of `key` unless it is there.
    fn ensure(&mut self, device: &wgpu::Device, key: (Blend, Option<Kind>)) {
        if self.pipelines.get(key).is_some() {
            return;
        }
        let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ddi-painter-pipeline"),
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
                    format: self.format,
                    blend: Some(crate::textured::blend_state(key.0)),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: key.1.map(stencil::state),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key, p);
    }
}

fn texture(
    device: &wgpu::Device,
    [width, height]: [u32; 2],
    format: wgpu::TextureFormat,
    label: &str,
    levels: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        // Copies from the browser's images and canvases need it to be a
        // render target too.
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    texture: &wgpu::Texture,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ddi-painter-texture-bg"),
        layout,
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
}

/// The run cache's pages on the device, drawn by the platform's
/// rasterizer.
struct Sink<'a> {
    raster: &'a mut dyn RunRasterizer,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    gpu: &'a mut Gpu,
}

impl PageSink for Sink<'_> {
    fn measure(&mut self, text: &str, style: &TextStyle, scale: f32) -> RunMetrics {
        self.raster.measure(text, style, scale)
    }

    fn draw(&mut self, text: &str, style: &TextStyle, scale: f32, pen: [f32; 2], cell: PageRect) {
        self.raster.draw(text, style, scale, pen, cell);
    }

    fn flush(&mut self, page: usize, rect: PageRect) {
        if let Some(Some(p)) = self.gpu.pages.get(page) {
            self.raster.upload(self.queue, &p.texture, rect);
        }
    }

    fn add_page(&mut self, page: usize) {
        let t = texture(
            self.device,
            [PAGE, PAGE],
            wgpu::TextureFormat::R8Unorm,
            "ddi-text-page",
            1,
        );
        let group = bind(self.device, &self.gpu.texture_layout, &self.gpu.sampler, &t);
        if self.gpu.pages.len() <= page {
            self.gpu.pages.resize_with(page + 1, || None);
        }
        self.gpu.pages[page] = Some(Page { texture: t, group });
    }

    fn drop_page(&mut self, page: usize) {
        if let Some(p) = self.gpu.pages.get_mut(page).and_then(Option::take) {
            p.texture.destroy();
        }
    }
}

/// Counters for the debug state.
#[derive(Clone, Copy, Debug, Default)]
pub struct PainterStats {
    pub textures: TextureStats,
    pub text: TextStats,
    /// The last frame's instances, draws and passes.
    pub instances: usize,
    pub draws: usize,
    pub passes: usize,
    /// Images the shell draws itself, set now
    /// ([`Painter::set_external_image`]).
    pub externals: usize,
}

pub struct Painter {
    format: wgpu::TextureFormat,
    gpu: Gpu,
    textures: TextureCache<Image>,
    text: RunCache,
    raster: Option<Box<dyn RunRasterizer>>,
    fonts_version: u64,
    /// Images whose mipmaps the next draw fills.
    unmipped: Vec<TextureId>,
    /// Images the shell draws itself, by source, and their bindings
    /// (`None`: none yet, the placeholder shows).
    externals: Vec<(Arc<str>, Option<wgpu::BindGroup>)>,
    /// The last frame's, kept for their allocations.
    instances: Vec<Instance>,
    steps: Vec<Step>,
    last: (usize, usize, usize),
    /// Drawn since the last [`Painter::submitted`]: another draw before
    /// the submit belongs to the same frame (a page the first one samples
    /// must not be cleared for it).
    drawing: bool,
}

impl Painter {
    /// A painter drawing to `format`: a plain (non-sRGB) view of the
    /// target, where colours are blended as given, encoded (the menus');
    /// or an sRGB one (a play's surface on WebGL2, which has no plain
    /// view), where they are decoded first and blended in linear light, as
    /// the scene's are: the same colours, the edges of text and of soft
    /// shapes a little different.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Painter {
        Painter {
            format,
            gpu: Gpu::new(device, queue, format),
            textures: TextureCache::new(TEXTURE_BUDGET),
            text: RunCache::new(PAGE, TEXT_BUDGET),
            raster: None,
            fonts_version: 0,
            unmipped: Vec::new(),
            externals: Vec::new(),
            instances: Vec::new(),
            steps: Vec::new(),
            last: (0, 0, 0),
            drawing: false,
        }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The device was lost and this is the new one (to draw to `format`):
    /// every texture and run is gone, ids stay, and they are loaded and
    /// drawn again as they are shown.
    pub fn device_changed(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        self.textures.unload_all();
        self.unmipped.clear();
        for e in &mut self.externals {
            e.1 = None;
        }
        self.text.reset();
        self.drawing = false;
        self.format = format;
        self.gpu = Gpu::new(device, queue, format);
    }

    /// What draws the text runs (none: text is not drawn).
    pub fn set_rasterizer(&mut self, raster: Box<dyn RunRasterizer>) {
        self.raster = Some(raster);
        self.text.clear();
    }

    /// Bytes of textures and of text pages to keep (more while a frame
    /// needs them).
    pub fn set_budgets(&mut self, textures: u64, text: u64) {
        self.textures.set_budget(textures);
        self.text.set_budget(text);
    }

    /// Lets go of the textures of the sources `which` picks
    /// ([`TextureCache::forget`]).
    pub fn forget_textures(&mut self, which: impl Fn(&str) -> bool) {
        // Dropped, not destroyed: a draw not submitted yet may sample them.
        drop(self.textures.forget(which));
    }

    /// The id of a texture of `key`, the same every time.
    pub fn texture(&mut self, key: TextureKey) -> TextureId {
        self.textures.id(key)
    }

    /// Textures drawn and not loaded: the platform loads each and answers
    /// with [`Painter::insert_texture`] or [`Painter::fail_texture`].
    pub fn take_texture_requests(&mut self) -> Vec<(TextureId, TextureKey)> {
        self.textures.take_requests()
    }

    /// The longest texture side the device takes.
    pub fn max_texture_side(&self) -> u32 {
        self.gpu.max_side
    }

    /// Makes texture `id`, `size` pixels of encoded RGBA, and has `fill`
    /// write its first level through the queue (the platform's copy of a
    /// decoded image); the next draw fills its mipmaps.
    pub fn insert_texture(
        &mut self,
        device: &wgpu::Device,
        id: TextureId,
        size: [u32; 2],
        fill: impl FnOnce(&wgpu::Texture),
    ) {
        let levels = crate::mipmaps::levels(size[0], size[1]);
        let t = texture(
            device,
            size,
            wgpu::TextureFormat::Rgba8Unorm,
            "ddi-painter-image",
            levels,
        );
        fill(&t);
        let group = bind(device, &self.gpu.texture_layout, &self.gpu.sampler, &t);
        // The mipmaps add a third.
        let bytes = u64::from(size[0].max(1)) * u64::from(size[1].max(1)) * 4;
        let bytes = if levels > 1 { bytes * 4 / 3 } else { bytes };
        if levels > 1 {
            self.unmipped.push(id);
        }
        // What it replaces or the budget evicts is dropped, not destroyed
        // (a draw recorded and not submitted may sample it).
        drop(self.textures.insert(id, Image { texture: t, group }, bytes));
    }

    /// [`Painter::insert_texture`] from encoded RGBA rows.
    pub fn insert_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: TextureId,
        size: [u32; 2],
        rgba: &[u8],
    ) {
        self.insert_texture(device, id, size, |t| {
            queue.write_texture(
                t.as_image_copy(),
                rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size[0] * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
            );
        });
    }

    /// Draws the images of `source` from `texture` (a plain, filterable
    /// texture of encoded values the shell draws into each frame: the
    /// options' field preview, `crate::field_preview`), whatever size they
    /// are asked at, never loaded by the platform; `None` shows their
    /// placeholder. Set again when the texture is made again (a new size);
    /// what it draws shows in the draws after it is drawn into. Gone with
    /// the device ([`Painter::device_changed`]).
    pub fn set_external_image(
        &mut self,
        device: &wgpu::Device,
        source: &str,
        texture: Option<&wgpu::Texture>,
    ) {
        // Loads asked of the platform before are not waited for.
        self.textures.cancel(source);
        let group = texture.map(|t| bind(device, &self.gpu.texture_layout, &self.gpu.sampler, t));
        match self.externals.iter_mut().find(|e| &*e.0 == source) {
            Some(e) => e.1 = group,
            None => self.externals.push((source.into(), group)),
        }
    }

    /// The platform could not load texture `id`: its placeholder stays.
    pub fn fail_texture(&mut self, id: TextureId) {
        self.textures.fail(id);
    }

    /// Measures text in units drawn at `scale` physical pixels each.
    pub fn measure(&mut self, scale: f32) -> PainterMeasure<'_> {
        self.sync_fonts();
        PainterMeasure {
            text: &mut self.text,
            raster: self.raster.as_deref_mut(),
            scale,
        }
    }

    pub fn stats(&self) -> PainterStats {
        PainterStats {
            textures: self.textures.stats(),
            text: self.text.stats(),
            instances: self.last.0,
            draws: self.last.1,
            passes: self.last.2,
            externals: self.externals.iter().filter(|e| e.1.is_some()).count(),
        }
    }

    /// The frame's draws were submitted: the next draw starts a frame.
    pub fn submitted(&mut self) {
        self.drawing = false;
    }

    /// Runs drawn with fonts that changed since are drawn again.
    fn sync_fonts(&mut self) {
        if let Some(v) = self.raster.as_ref().map(|r| r.fonts_version())
            && v != self.fonts_version
        {
            self.fonts_version = v;
            self.text.clear();
        }
    }

    /// Draws `list` into `target` (`size` pixels, `scale` pixels a unit),
    /// cleared to `clear` first, else drawn over. Draws until
    /// [`Painter::submitted`] make one frame (pages and textures they use
    /// stay). At each
    /// [`Command::External`] the pass ends and `external` records the
    /// shell's own drawing into `encoder`; the list goes on over it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        scale: f32,
        clear: Option<wgpu::Color>,
        list: &DrawList,
        external: &mut dyn FnMut(u32, &mut wgpu::CommandEncoder),
    ) {
        // A new frame, unless the last draw was not submitted yet.
        let new_frame = !std::mem::replace(&mut self.drawing, true);
        if new_frame {
            self.sync_fonts();
            self.textures.begin_frame();
            self.text.begin_frame();
        }
        let Painter {
            gpu,
            textures,
            text,
            raster,
            instances,
            steps,
            unmipped,
            externals,
            ..
        } = self;
        instances.clear();
        steps.clear();
        if let Some(raster) = raster.as_deref_mut().filter(|_| new_frame) {
            text.compact(&mut Sink {
                raster,
                device,
                queue,
                gpu,
            });
        }
        let s = scale.max(1e-3);
        let with_stencil = uses_stencil(list);
        let mut clips = Clips::new(with_stencil, s, size);
        let linear = |m: [f32; 6]| {
            (
                [m[0] * s, m[1] * s, m[2] * s, m[3] * s],
                [m[4] * s, m[5] * s],
            )
        };
        // A clip's quad, untextured, drawing nothing but the stencil.
        let clip_quad = |corners: &[[f32; 2]; 4]| {
            let p = corners.map(|c| [c[0] * s, c[1] * s]);
            Instance {
                axes: [p[0][0], p[0][1], p[1][0], p[1][1]],
                origin: [0.0; 2],
                mode: UNTEXTURED | PROJECTED,
                _pad: 0,
                rect: [p[2][0], p[2][1], p[3][0], p[3][1]],
                uv: [0.0, 0.0, 1.0, 1.0],
                // Opaque, so the alpha test keeps it; it draws no colour.
                colors: [u32::MAX; 4],
                w: [1.0; 4],
                shape: [0.0; 4],
                shape2: NO_SHAPE2,
                blur: [0.0; 4],
            }
        };
        for command in &list.commands {
            match command {
                Command::PushClip(r) => clips.push(*r),
                Command::PushClipQuad { corners, bound } => {
                    if let Some((sc, stencil)) = clips.push_quad(corners, *bound) {
                        add(
                            steps,
                            instances.len() as u32,
                            Bind::Blank,
                            Blend::NoEffect,
                            sc,
                            Some(stencil),
                        );
                        instances.push(clip_quad(corners));
                    }
                }
                Command::PopClip => {
                    if let Some((corners, sc, stencil)) = clips.pop() {
                        add(
                            steps,
                            instances.len() as u32,
                            Bind::Blank,
                            Blend::NoEffect,
                            sc,
                            Some(stencil),
                        );
                        instances.push(clip_quad(&corners));
                    }
                }
                Command::External(slot) => steps.push(Step::External(*slot)),
                Command::Quad(q) => {
                    let Some((sc, stencil)) = clips.draw(q.mask) else {
                        continue;
                    };
                    let (bind, mode, colors) = match q.texture {
                        None => (Bind::Blank, UNTEXTURED, q.colors),
                        // An image the shell draws (never loaded), else its
                        // own size, or another of the same source.
                        Some(id) => {
                            let drawn = match external_image(externals, textures, id) {
                                Some(Some(i)) => Some(Bind::External(i)),
                                Some(None) => None,
                                None => textures.resolve(id).map(Bind::Image),
                            };
                            match drawn {
                                Some(bind) => (bind, 0, q.colors),
                                None => {
                                    let [r, g, b] = PLACEHOLDER;
                                    (Bind::Blank, UNTEXTURED, q.colors.map(|c| [r, g, b, c[3]]))
                                }
                            }
                        }
                    };
                    let mode = mode | if q.glow { GLOW } else { 0 };
                    let instance = match &q.projected {
                        Some(p) => {
                            let c = p.corners.map(|c| [c[0] * s, c[1] * s]);
                            Instance {
                                axes: [c[0][0], c[0][1], c[1][0], c[1][1]],
                                origin: [0.0; 2],
                                mode: mode | PROJECTED,
                                _pad: 0,
                                rect: [c[2][0], c[2][1], c[3][0], c[3][1]],
                                uv: q.uv,
                                colors: colors.map(pack),
                                w: p.w,
                                shape: [0.0; 4],
                                shape2: NO_SHAPE2,
                                blur: [0.0; 4],
                            }
                        }
                        None => {
                            let (axes, origin) = linear(q.transform.0);
                            let sh = &q.shape;
                            let mode = mode
                                | if sh.is_cut() { CUT } else { 0 }
                                | if sh.saturation != 1.0 { SATURATION } else { 0 }
                                | if q.blur != [0.0; 2] { BLUR } else { 0 };
                            Instance {
                                axes,
                                origin,
                                mode,
                                _pad: 0,
                                rect: q.rect,
                                uv: q.uv,
                                colors: colors.map(pack),
                                w: [1.0; 4],
                                shape: [sh.radius[0], sh.radius[1], sh.soft[0], sh.soft[1]],
                                shape2: [sh.outline[0], sh.outline[1], sh.saturation, 0.0],
                                blur: [q.blur[0], q.blur[1], 0.0, 0.0],
                            }
                        }
                    };
                    add(steps, instances.len() as u32, bind, q.blend, sc, stencil);
                    instances.push(instance);
                }
                Command::Text(t) => {
                    let (Some((sc, stencil)), Some(raster)) =
                        (clips.draw(t.mask), raster.as_deref_mut())
                    else {
                        continue;
                    };
                    let mut sink = Sink {
                        raster,
                        device,
                        queue,
                        gpu,
                    };
                    // Drawn at the size it is shown (the transform's
                    // scale, in quarter steps of an octave), so a zoomed
                    // run stays sharp; turned ones are sampled.
                    let zoom = run_zoom(&t.transform);
                    let r = s * zoom;
                    let Some(placed) = text.place(&t.text, &t.style, r, &mut sink) else {
                        continue;
                    };
                    let cell = placed.cell;
                    let (w, h) = (cell.width as f32, cell.height as f32);
                    // In the run's own units.
                    let baseline = placed.metrics.baseline / r;
                    let m = t.transform.0;
                    let one_for_one = m[1] == 0.0 && m[2] == 0.0 && m[0] == zoom && m[3] == zoom;
                    let x0 = -placed.pen[0] / r;
                    let y0 = baseline - placed.pen[1] / r;
                    let cell_rect = [x0, y0, x0 + w / r, y0 + h / r];
                    let blurred = t.blur != [0.0; 2];
                    let (axes, origin, rect, extra, ws, blur) = if let Some(view) = &t.perspective {
                        let Some((top, bottom, ws)) = project_rect(view, cell_rect, s) else {
                            continue;
                        };
                        (top, [0.0; 2], bottom, PROJECTED, ws, [0.0; 2])
                    } else if one_for_one {
                        // The page's texels one for one: the pen on a
                        // whole pixel; the blur in pixels.
                        let pen = t.transform.apply([0.0, baseline]);
                        let x = (pen[0] * s).round() - placed.pen[0];
                        let y = (pen[1] * s).round() - placed.pen[1];
                        let k = zoom * s;
                        (
                            [1.0, 0.0, 0.0, 1.0],
                            [x, y],
                            [0.0, 0.0, w, h],
                            if blurred { BLUR } else { 0 },
                            [1.0; 4],
                            [t.blur[0] * k, t.blur[1] * k],
                        )
                    } else {
                        let (axes, origin) = linear(m);
                        (
                            axes,
                            origin,
                            cell_rect,
                            if blurred { BLUR } else { 0 },
                            [1.0; 4],
                            t.blur,
                        )
                    };
                    let p = PAGE as f32;
                    add(
                        steps,
                        instances.len() as u32,
                        Bind::Page(placed.page),
                        t.blend,
                        sc,
                        stencil,
                    );
                    instances.push(Instance {
                        axes,
                        origin,
                        mode: COVERAGE | extra,
                        _pad: 0,
                        rect,
                        uv: [
                            cell.x as f32 / p,
                            cell.y as f32 / p,
                            cell.right() as f32 / p,
                            cell.bottom() as f32 / p,
                        ],
                        colors: [pack(t.color); 4],
                        w: ws,
                        shape: [0.0; 4],
                        shape2: NO_SHAPE2,
                        blur: [blur[0], blur[1], 0.0, 0.0],
                    });
                }
            }
        }
        if let Some(raster) = raster.as_deref_mut() {
            let mut sink = Sink {
                raster,
                device,
                queue,
                gpu,
            };
            text.flush(&mut sink);
            text.trim(PAGE_TRIM_FRAMES, &mut sink);
        }
        // Dropped, not destroyed: what this frame's encoder (or one not
        // submitted yet) uses stays alive until done with.
        drop(textures.evict());
        // Images loaded since, and still kept: their mipmaps, recorded
        // before the passes that sample them.
        for id in unmipped.drain(..) {
            if let Some(image) = textures.peek(id) {
                gpu.mipmaps.record(
                    device,
                    encoder,
                    &image.texture,
                    wgpu::TextureFormat::Rgba8Unorm,
                );
            }
        }
        gpu.instances
            .upload(device, queue, bytemuck::cast_slice(instances));
        let g = Globals {
            scale: [2.0 / size[0].max(1) as f32, -2.0 / size[1].max(1) as f32],
            translate: [-1.0, 1.0],
            linear: if self.format.is_srgb() { 1.0 } else { 0.0 },
            _pad: [0.0; 3],
        };
        queue.write_buffer(&gpu.globals, 0, bytemuck::bytes_of(&g));

        for step in steps.iter() {
            if let Step::Draw(b) = step {
                gpu.ensure(device, (b.blend, b.stencil.map(|s| s.0)));
            }
        }
        let stencil_view = with_stencil.then(|| gpu.stencil.view(device, size));
        let (mut draws, mut passes) = (0, 0);
        let mut first = true;
        let mut at = 0;
        loop {
            let end = steps[at..]
                .iter()
                .position(|s| matches!(s, Step::External(_)))
                .map_or(steps.len(), |p| at + p);
            let load = match (first, clear) {
                (true, Some(c)) => Some(wgpu::LoadOp::Clear(c)),
                _ if end > at => Some(wgpu::LoadOp::Load),
                _ => None,
            };
            if let Some(load) = load {
                passes += 1;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ddi-painter"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    // Cleared in the frame's first pass, kept after.
                    depth_stencil_attachment: stencil_view
                        .map(|v| stencil::Attachment::pass(v, passes == 1)),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, &gpu.globals_group, &[]);
                let mut blend = None;
                let mut reference = None;
                let mut scissor_now = None;
                for step in &steps[at..end] {
                    let Step::Draw(b) = step else { continue };
                    let group = match b.bind {
                        Bind::Blank => Some(&gpu.blank),
                        Bind::Image(id) => textures.peek(id).map(|i| &i.group),
                        Bind::External(i) => externals.get(i).and_then(|e| e.1.as_ref()),
                        Bind::Page(p) => {
                            gpu.pages.get(p).and_then(Option::as_ref).map(|p| &p.group)
                        }
                    };
                    let Some(group) = group else { continue };
                    let key = (b.blend, b.stencil.map(|s| s.0));
                    if blend != Some(key) {
                        let Some(p) = gpu.pipelines.get(key) else {
                            continue;
                        };
                        pass.set_pipeline(p);
                        blend = Some(key);
                    }
                    if let Some((_, r)) = b.stencil
                        && reference != Some(r)
                    {
                        pass.set_stencil_reference(r);
                        reference = Some(r);
                    }
                    if scissor_now != Some(b.scissor) {
                        let [x, y, w, h] = b.scissor;
                        pass.set_scissor_rect(x, y, w, h);
                        scissor_now = Some(b.scissor);
                    }
                    pass.set_bind_group(1, group, &[]);
                    for (buffer, range) in gpu.instances.pieces(b.start..b.end) {
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..6, range);
                        draws += 1;
                    }
                }
            }
            first = false;
            match steps.get(end) {
                Some(Step::External(slot)) => {
                    external(*slot, encoder);
                    at = end + 1;
                }
                _ => break,
            }
        }
        self.last = (self.instances.len(), draws, passes);
    }
}

/// The external image texture `id` is drawn from: `Some(Some(i))` with
/// one, `Some(None)` a source the shell draws that has none now, `None` an
/// ordinary texture.
fn external_image(
    externals: &[(Arc<str>, Option<wgpu::BindGroup>)],
    textures: &TextureCache<Image>,
    id: TextureId,
) -> Option<Option<usize>> {
    if externals.is_empty() {
        return None;
    }
    let key = textures.key(id)?;
    let i = externals.iter().position(|e| e.0 == key.source)?;
    Some(externals[i].1.is_some().then_some(i))
}

/// [`Painter::measure`]: measures runs as the painter draws them, through
/// its cache; without a rasterizer, [`FixedMeasure`].
pub struct PainterMeasure<'a> {
    text: &'a mut RunCache,
    raster: Option<&'a mut (dyn RunRasterizer + 'static)>,
    scale: f32,
}

impl TextMeasure for PainterMeasure<'_> {
    fn measure(&mut self, text: &str, style: &TextStyle) -> RunMetrics {
        let Some(raster) = self.raster.as_deref_mut() else {
            return FixedMeasure.measure(text, style);
        };
        let mut f = |t: &str, st: &TextStyle, k: f32| raster.measure(t, st, k);
        Measurer {
            cache: self.text,
            measure: &mut f,
            scale: self.scale,
        }
        .measure(text, style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_validates() {
        let source = include_str!("painter.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("WGSL validates");
    }

    /// WebGL2 binds uniforms in multiples of 16 bytes, and the shader's
    /// globals are the struct written to them.
    #[test]
    fn the_globals_are_whole_sixteens_on_both_sides() {
        let module =
            wgpu::naga::front::wgsl::parse_str(include_str!("painter.wgsl")).expect("parses");
        let (_, ty) = module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref() == Some("Globals"))
            .expect("Globals");
        let wgpu::naga::TypeInner::Struct { span, .. } = ty.inner else {
            panic!("{ty:?}")
        };
        assert_eq!(span as usize, std::mem::size_of::<Globals>());
        assert_eq!(span % 16, 0);
    }

    #[test]
    fn runs_are_drawn_at_their_zoom_in_quarter_octaves() {
        use crate::draw2d::Affine;
        assert_eq!(run_zoom(&Affine::translate(3.0, 4.0)), 1.0);
        assert_eq!(run_zoom(&Affine::scale(2.0, 2.0)), 2.0);
        // Turning keeps the size.
        assert!((run_zoom(&Affine::rotate(0.7).then(Affine::scale(2.0, 2.0))) - 2.0).abs() < 1e-6);
        // 1.1 is nearer 2^(1/4) than 1.
        assert_eq!(run_zoom(&Affine::scale(1.1, 1.1)), 0.25f32.exp2());
        assert_eq!(run_zoom(&Affine::scale(100.0, 100.0)), 8.0);
        assert_eq!(run_zoom(&Affine::scale(0.0, 1.0)), 1.0);
    }

    #[test]
    fn colours_pack_as_bytes_in_order() {
        assert_eq!(pack([1.0, 0.0, 0.5, 1.0]).to_le_bytes(), [255, 0, 128, 255]);
        assert_eq!(pack([2.0, -1.0, 0.0, 0.0]), 255);
    }

    #[test]
    fn draws_join_while_nothing_changes() {
        let full = [0, 0, 100, 100];
        let half = [0, 0, 50, 100];
        let mut steps = Vec::new();
        add(&mut steps, 0, Bind::Blank, Blend::Normal, full, None);
        add(&mut steps, 1, Bind::Blank, Blend::Normal, full, None);
        add(&mut steps, 2, Bind::Page(0), Blend::Normal, full, None);
        add(&mut steps, 3, Bind::Page(0), Blend::Add, full, None);
        add(&mut steps, 4, Bind::Page(0), Blend::Add, half, None);
        steps.push(Step::External(7));
        // Not after the slot, though everything else matches.
        add(&mut steps, 5, Bind::Page(0), Blend::Add, half, None);
        let ranges: Vec<_> = steps
            .iter()
            .map(|s| match s {
                Step::Draw(b) => (b.start, b.end),
                Step::External(n) => (100 + n, 0),
            })
            .collect();
        assert_eq!(
            ranges,
            vec![(0, 2), (2, 3), (3, 4), (4, 5), (107, 0), (5, 6)]
        );
    }

    #[test]
    fn masks_and_turned_clips_need_the_stencil() {
        let mut l = DrawList::new();
        l.rect([0.0, 0.0, 1.0, 1.0], [1.0; 4]);
        l.push_clip([0.0, 0.0, 1.0, 1.0]);
        assert!(!uses_stencil(&l));
        l.push(Command::PushClipQuad {
            corners: [[0.0; 2]; 4],
            bound: [0.0; 4],
        });
        assert!(uses_stencil(&l));
        let mut m = DrawList::new();
        m.quad(crate::draw2d::Quad {
            mask: Mask::Outside,
            ..crate::draw2d::Quad::plain([0.0; 4], [1.0; 4])
        });
        assert!(uses_stencil(&m));
    }

    #[test]
    fn runs_in_perspective_are_projected_by_their_corners() {
        let camera = ddi_actor::Perspective {
            vanish: [100.0, 100.0],
            distance: 400.0,
            near: 1.0,
            far: 1400.0,
        };
        // The run's space 100 units nearer the eye, at the vanishing point.
        let mut m = [0.0; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[12] = 100.0;
        m[13] = 100.0;
        m[14] = 100.0;
        m[15] = 1.0;
        let (top, bottom, w) = project_rect(&(m, camera), [0.0, 0.0, 30.0, 10.0], 2.0).unwrap();
        // 4/3 as big, two pixels a unit.
        let k = 400.0 / 300.0;
        assert!((top[0] - 200.0).abs() < 1e-3 && (top[2] - (100.0 + 30.0 * k) * 2.0).abs() < 1e-3);
        assert!((bottom[1] - (100.0 + 10.0 * k) * 2.0).abs() < 1e-3);
        assert!(w.iter().all(|&w| (w - 0.75).abs() < 1e-6));
        // Behind the eye: not drawn.
        m[14] = 500.0;
        assert!(project_rect(&(m, camera), [0.0, 0.0, 1.0, 1.0], 1.0).is_none());
    }

    #[test]
    fn clips_count_levels_in_the_stencil_and_take_them_back() {
        let quad = [[10.0, 0.0], [20.0, 10.0], [10.0, 20.0], [0.0, 10.0]];
        let mut c = Clips::new(true, 2.0, [100, 100]);
        assert_eq!(
            c.draw(Mask::None),
            Some(([0, 0, 100, 100], Some((Kind::Level, 0))))
        );
        // A turned clip counts where it is, at level 0, within the target.
        assert_eq!(
            c.push_quad(&quad, [0.0, 0.0, 20.0, 20.0]),
            Some(([0, 0, 100, 100], (Kind::ClipIn, 0)))
        );
        // Inside: level 1 and its bound's scissor; a mask tested there.
        assert_eq!(
            c.draw(Mask::None),
            Some(([0, 0, 40, 40], Some((Kind::Level, 1))))
        );
        assert_eq!(
            c.draw(Mask::Inside),
            Some(([0, 0, 40, 40], Some((Kind::Masked, 0x81))))
        );
        // A rectangle inside it counts nothing; a second quad counts
        // within the first's scissor.
        c.push([5.0, 5.0, 50.0, 50.0]);
        assert_eq!(
            c.push_quad(&quad, [0.0, 0.0, 15.0, 15.0]),
            Some(([10, 10, 30, 30], (Kind::ClipIn, 1)))
        );
        // A clear of the mask covers the whole target.
        assert_eq!(
            c.draw(Mask::Clear),
            Some(([0, 0, 100, 100], Some((Kind::MaskClear, 0))))
        );
        // Popped: each counted quad again, at its own level, with the
        // scissor it counted within.
        assert_eq!(c.pop(), Some((quad, [10, 10, 30, 30], (Kind::ClipOut, 2))));
        assert_eq!(c.pop(), None);
        assert_eq!(c.pop(), Some((quad, [0, 0, 100, 100], (Kind::ClipOut, 1))));
        assert_eq!(
            c.draw(Mask::None),
            Some(([0, 0, 100, 100], Some((Kind::Level, 0))))
        );
        // Without the stencil, a quad clips by its bound alone.
        let mut plain = Clips::new(false, 1.0, [100, 100]);
        assert_eq!(plain.push_quad(&quad, [0.0, 0.0, 20.0, 20.0]), None);
        assert_eq!(plain.draw(Mask::None), Some(([0, 0, 20, 20], None)));
        assert_eq!(plain.pop(), None);
    }
}
