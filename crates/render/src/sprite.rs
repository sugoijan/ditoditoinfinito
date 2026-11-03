//! Instanced procedural-quad pipeline.

use bytemuck::{Pod, Zeroable};

/// Shape ids understood by `shader.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Shape {
    Rect = 0,
    /// Infinito's mine, made light: an eight-pointed band turning round a
    /// red core.
    Mine = 3,
    Circle = 4,
    /// A long note's body: `params` its roll (1) or hold (0), its state
    /// (0 waiting, 1 held, 2 dropped), its ribs' way (1 to its tail at
    /// the quad's +y, -1 to its head, 0 flat) and the lane's pixels; with
    /// [`HOLD_TAIL`] its tail end pointed.
    HoldBody = 5,
}

/// [`Shape::HoldBody`]'s bit: the body ends at its tail here.
pub const HOLD_TAIL: u32 = 1 << 8;

/// A built-in note or receptor (`docs/PLAN.md` decision 80): Infinito's
/// shapes, drawn by `shader.wgsl` from `GLYPH_BASE` on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Glyph {
    Arrow = 32,
    /// Pump's centre panel, and other square lanes.
    Center = 33,
    Onigiri = 34,
    /// Any other lane: a ring.
    Dot = 35,
}

/// How a [`Glyph`]'s band is painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Paint {
    /// Infinito's palette, its window at `params[0]` cycles (Vivid, Flat).
    Palette = 0,
    /// `color` to `params[0..3]` in waves shifted by `params[3]` cycles
    /// (Rainbow).
    Flow = 1,
    /// `color`, shimmering (Quantized, one color, the chart's colors, a
    /// glow).
    Solid = 2,
    /// White and mint: a lift.
    Lift = 3,
    /// A long note's waves (its head): `params` as [`Shape::HoldBody`]'s.
    Long = 4,
    /// A receptor's neutral band: `params[0]` 1 on the beat's bright part,
    /// `params[1]` a judgment's flash in `color`.
    Receptor = 5,
}

/// A glyph's quad, in arrow widths: room for its shadow round the arrow
/// (`GLYPH_UNITS` of Infinito's SVG units in half of it, the arrow 48.75
/// wide).
pub const GLYPH_QUAD: f32 = 1.567;

impl Glyph {
    /// The instance shape id of this glyph painted with `paint`, showing
    /// `charge`'s frame (0 full, on the hit; 1 empty; 2 and 3 filling).
    pub fn shape(self, paint: Paint, charge: Option<u32>) -> u32 {
        self as u32 | (paint as u32) << 8 | charge.map_or(0, |c| (c % 4) + 1) << 12
    }

    /// The glyph and paint of an instance's `shape`, if it is one.
    pub fn of(shape: u32) -> Option<(Glyph, Paint)> {
        let glyph = match shape & 0xff {
            32 => Glyph::Arrow,
            33 => Glyph::Center,
            34 => Glyph::Onigiri,
            35 => Glyph::Dot,
            _ => return None,
        };
        let paint = match (shape >> 8) & 0xf {
            0 => Paint::Palette,
            1 => Paint::Flow,
            2 => Paint::Solid,
            3 => Paint::Lift,
            4 => Paint::Long,
            _ => Paint::Receptor,
        };
        Some((glyph, paint))
    }

    /// The charge frame an instance's `shape` shows.
    pub fn charge(shape: u32) -> Option<u32> {
        ((shape >> 12) & 0x7).checked_sub(1)
    }
}

/// Whether an instance's `params[0..3]` is a color (the renderer moves
/// it to the target's space as it does `color`).
pub fn params_are_color(shape: u32) -> bool {
    matches!(Glyph::of(shape), Some((_, Paint::Flow)))
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Instance {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub rotation: f32,
    pub shape: u32,
    pub color: [f32; 4],
    pub params: [f32; 4],
}

impl Instance {
    pub fn new(center: [f32; 2], size: [f32; 2], shape: Shape, color: [f32; 4]) -> Instance {
        Instance {
            center,
            size,
            rotation: 0.0,
            shape: shape as u32,
            color,
            params: [0.0; 4],
        }
    }

    pub fn rotated(mut self, radians: f32) -> Instance {
        self.rotation = radians;
        self
    }

    pub fn params(mut self, params: [f32; 4]) -> Instance {
        self.params = params;
        self
    }

    const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Uint32, 4 => Float32x4, 5 => Float32x4
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Globals {
    scale: [f32; 2],
    translate: [f32; 2],
    time: f32,
    /// 1 when the target is sRGB: the shader's own color maths then works
    /// in linear light.
    srgb: f32,
    _pad: [f32; 2],
}

pub struct SpritePipeline {
    pipeline: wgpu::RenderPipeline,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: crate::instances::InstanceBuffers,
    srgb: bool,
}

impl SpritePipeline {
    /// Sprites drawn to `format`, whose values are linear light when
    /// `linear` (an sRGB target, or a float one standing in for it).
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, linear: bool) -> SpritePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-sprite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let (globals, bgl, bind_group) = crate::gpu::uniform_group(
            device,
            "ddi-globals",
            std::mem::size_of::<Globals>(),
            wgpu::ShaderStages::VERTEX_FRAGMENT,
        );
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-sprite-layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = Self::pipeline(device, &shader, &layout, format);
        let instances = crate::instances::InstanceBuffers::new(
            device,
            "ddi-instances",
            std::mem::size_of::<Instance>(),
        );
        SpritePipeline {
            pipeline,
            shader,
            layout,
            globals,
            bind_group,
            instances,
            srgb: linear,
        }
    }

    /// Draws to `format` from now on (its values linear light as before).
    pub fn retarget(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.pipeline = Self::pipeline(device, &self.shader, &self.layout, format);
    }

    fn pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ddi-sprite-pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(Instance::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
    }

    /// Upload globals for a target of `width × height` pixels, origin top-left,
    /// y down.
    pub fn set_viewport(&self, queue: &wgpu::Queue, width: f32, height: f32, time: f32) {
        let g = Globals {
            scale: [2.0 / width.max(1.0), -2.0 / height.max(1.0)],
            translate: [-1.0, 1.0],
            time,
            srgb: if self.srgb { 1.0 } else { 0.0 },
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
    }

    /// Upload instances; grows the buffers when needed.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, instances: &[Instance]) {
        self.instances
            .upload(device, queue, bytemuck::cast_slice(instances));
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, count: usize) {
        self.draw_range(pass, 0..count as u32);
    }

    /// Draws the uploaded instances `range`.
    pub fn draw_range<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, range: std::ops::Range<u32>) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        // Past one buffer's worth, the range is drawn from each buffer.
        for (buffer, range) in self.instances.pieces(range) {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..6, range);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Glyph, Paint, params_are_color};

    #[test]
    fn a_glyphs_shape_says_its_paint_and_charge() {
        for glyph in [Glyph::Arrow, Glyph::Center, Glyph::Onigiri, Glyph::Dot] {
            for paint in [
                Paint::Palette,
                Paint::Flow,
                Paint::Solid,
                Paint::Lift,
                Paint::Long,
                Paint::Receptor,
            ] {
                for charge in [None, Some(0), Some(3)] {
                    let shape = glyph.shape(paint, charge);
                    assert_eq!(Glyph::of(shape), Some((glyph, paint)));
                    assert_eq!(Glyph::charge(shape), charge);
                    assert_eq!(params_are_color(shape), paint == Paint::Flow);
                }
            }
        }
        assert_eq!(Glyph::of(super::Shape::Mine as u32), None);
    }

    /// The shader parses and validates (the browser would only say so at
    /// run time, on the player's machine).
    #[test]
    fn shader_validates() {
        let source = include_str!("shader.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("WGSL validates");
    }
}
