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

use bytemuck::{Pod, Zeroable};
use ddi_actor::text::{FixedMeasure, RunMetrics, TextMeasure, TextStyle};

use crate::atlas::PageRect;
use crate::draw2d::{Blend, ClipStack, Command, DrawList, scissor};
use crate::instances::InstanceBuffers;
use crate::text_atlas::{Measurer, PAGE, PageSink, RunCache, RunRasterizer, TextStats};
use crate::texture_cache::{TextureCache, TextureId, TextureKey, TextureStats};

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

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Instance {
    axes: [f32; 4],
    origin: [f32; 2],
    mode: u32,
    _pad: u32,
    rect: [f32; 4],
    uv: [f32; 4],
    /// Top left, top right, bottom right, bottom left, as 8-bit RGBA.
    colors: [u32; 4],
}

const _: () = assert!(std::mem::size_of::<Instance>() == 80);

impl Instance {
    /// By the fields' offsets (`_pad` keeps `rect` 16-byte aligned).
    const ATTRIBUTES: [wgpu::VertexAttribute; 9] = {
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
        ]
    };
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Globals {
    scale: [f32; 2],
    translate: [f32; 2],
}

/// A colour as 8-bit RGBA (memory order r, g, b, a).
fn pack(c: [f32; 4]) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    b(c[0]) | b(c[1]) << 8 | b(c[2]) << 16 | b(c[3]) << 24
}

/// A texture of the cache on the device.
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
}

/// One draw: instances `start..end` with the same texture, blend and
/// scissor rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Batch {
    start: u32,
    end: u32,
    bind: Bind,
    blend: Blend,
    scissor: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Draw(Batch),
    External(u32),
}

/// Adds instance `index` to the last draw when it can join it.
fn add(steps: &mut Vec<Step>, index: u32, bind: Bind, blend: Blend, scissor: [u32; 4]) {
    if let Some(Step::Draw(b)) = steps.last_mut()
        && b.end == index
        && b.bind == bind
        && b.blend == blend
        && b.scissor == scissor
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
    }));
}

/// The device's objects; made again on a new device.
struct Gpu {
    pipelines: [wgpu::RenderPipeline; 4],
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    blank: wgpu::BindGroup,
    instances: InstanceBuffers,
    pages: Vec<Option<Page>>,
    max_side: u32,
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
        let pipelines = crate::textured::blend_states().map(|blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ddi-painter-pipeline"),
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
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ddi-painter-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white = texture(
            device,
            [1, 1],
            wgpu::TextureFormat::Rgba8Unorm,
            "ddi-painter-blank",
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
            pipelines,
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
        }
    }
}

fn texture(
    device: &wgpu::Device,
    [width, height]: [u32; 2],
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
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
}

pub struct Painter {
    format: wgpu::TextureFormat,
    gpu: Gpu,
    textures: TextureCache<Image>,
    text: RunCache,
    raster: Option<Box<dyn RunRasterizer>>,
    fonts_version: u64,
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
    /// A painter drawing to `format`, a plain (non-sRGB) view of the
    /// target: colours are blended as given, encoded.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Painter {
        Painter {
            format,
            gpu: Gpu::new(device, queue, format),
            textures: TextureCache::new(TEXTURE_BUDGET),
            text: RunCache::new(PAGE, TEXT_BUDGET),
            raster: None,
            fonts_version: 0,
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
    /// write it (the platform's copy of a decoded image).
    pub fn insert_texture(
        &mut self,
        device: &wgpu::Device,
        id: TextureId,
        size: [u32; 2],
        fill: impl FnOnce(&wgpu::Texture),
    ) {
        let t = texture(
            device,
            size,
            wgpu::TextureFormat::Rgba8Unorm,
            "ddi-painter-image",
        );
        fill(&t);
        let group = bind(device, &self.gpu.texture_layout, &self.gpu.sampler, &t);
        let bytes = u64::from(size[0].max(1)) * u64::from(size[1].max(1)) * 4;
        for freed in self.textures.insert(id, Image { texture: t, group }, bytes) {
            freed.texture.destroy();
        }
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
        let mut clips = ClipStack::default();
        let mut clip = scissor(None, s, size);
        let linear = |m: [f32; 6]| {
            (
                [m[0] * s, m[1] * s, m[2] * s, m[3] * s],
                [m[4] * s, m[5] * s],
            )
        };
        for command in &list.commands {
            match command {
                Command::PushClip(r) => {
                    clips.push(*r);
                    clip = scissor(clips.current(), s, size);
                }
                Command::PopClip => {
                    clips.pop();
                    clip = scissor(clips.current(), s, size);
                }
                Command::External(slot) => steps.push(Step::External(*slot)),
                Command::Quad(q) => {
                    let Some(sc) = clip else { continue };
                    let (bind, mode, colors) = match q.texture {
                        None => (Bind::Blank, UNTEXTURED, q.colors),
                        Some(id) if textures.get(id).is_some() => (Bind::Image(id), 0, q.colors),
                        Some(_) => {
                            let [r, g, b] = PLACEHOLDER;
                            (Bind::Blank, UNTEXTURED, q.colors.map(|c| [r, g, b, c[3]]))
                        }
                    };
                    let (axes, origin) = linear(q.transform.0);
                    add(steps, instances.len() as u32, bind, q.blend, sc);
                    instances.push(Instance {
                        axes,
                        origin,
                        mode,
                        _pad: 0,
                        rect: q.rect,
                        uv: q.uv,
                        colors: colors.map(pack),
                    });
                }
                Command::Text(t) => {
                    let (Some(sc), Some(raster)) = (clip, raster.as_deref_mut()) else {
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
                    let (axes, origin, rect) = if one_for_one {
                        // The page's texels one for one: the pen on a
                        // whole pixel.
                        let pen = t.transform.apply([0.0, baseline]);
                        let x = (pen[0] * s).round() - placed.pen[0];
                        let y = (pen[1] * s).round() - placed.pen[1];
                        ([1.0, 0.0, 0.0, 1.0], [x, y], [0.0, 0.0, w, h])
                    } else {
                        let (axes, origin) = linear(m);
                        let x0 = -placed.pen[0] / r;
                        let y0 = baseline - placed.pen[1] / r;
                        (axes, origin, [x0, y0, x0 + w / r, y0 + h / r])
                    };
                    let p = PAGE as f32;
                    add(
                        steps,
                        instances.len() as u32,
                        Bind::Page(placed.page),
                        t.blend,
                        sc,
                    );
                    instances.push(Instance {
                        axes,
                        origin,
                        mode: COVERAGE,
                        _pad: 0,
                        rect,
                        uv: [
                            cell.x as f32 / p,
                            cell.y as f32 / p,
                            cell.right() as f32 / p,
                            cell.bottom() as f32 / p,
                        ],
                        colors: [pack(t.color); 4],
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
        for freed in textures.evict() {
            freed.texture.destroy();
        }
        gpu.instances
            .upload(device, queue, bytemuck::cast_slice(instances));
        let g = Globals {
            scale: [2.0 / size[0].max(1) as f32, -2.0 / size[1].max(1) as f32],
            translate: [-1.0, 1.0],
        };
        queue.write_buffer(&gpu.globals, 0, bytemuck::bytes_of(&g));

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
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, &gpu.globals_group, &[]);
                let mut blend = None;
                let mut scissor_now = None;
                for step in &steps[at..end] {
                    let Step::Draw(b) = step else { continue };
                    let group = match b.bind {
                        Bind::Blank => Some(&gpu.blank),
                        Bind::Image(id) => textures.peek(id).map(|i| &i.group),
                        Bind::Page(p) => {
                            gpu.pages.get(p).and_then(Option::as_ref).map(|p| &p.group)
                        }
                    };
                    let Some(group) = group else { continue };
                    if blend != Some(b.blend) {
                        pass.set_pipeline(&gpu.pipelines[b.blend as usize]);
                        blend = Some(b.blend);
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
        add(&mut steps, 0, Bind::Blank, Blend::Normal, full);
        add(&mut steps, 1, Bind::Blank, Blend::Normal, full);
        add(&mut steps, 2, Bind::Page(0), Blend::Normal, full);
        add(&mut steps, 3, Bind::Page(0), Blend::Add, full);
        add(&mut steps, 4, Bind::Page(0), Blend::Add, half);
        steps.push(Step::External(7));
        // Not after the slot, though everything else matches.
        add(&mut steps, 5, Bind::Page(0), Blend::Add, half);
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
}
