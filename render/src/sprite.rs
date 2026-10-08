//! Instanced procedural-quad pipeline.

use bytemuck::{Pod, Zeroable};

/// Shape ids understood by `shader.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Shape {
    Rect = 0,
    Arrow = 1,
    ArrowOutline = 2,
    Mine = 3,
    Circle = 4,
    HoldBody = 5,
    /// An arrow with a flowing two-colour gradient: `color` to
    /// `params[0..3]` down the screen, shifted by `params[3]` cycles.
    ArrowGradient = 7,
    /// An arrow running from `color` at its tail to `params[0..3]` at its
    /// tip.
    ArrowRamp = 8,
}

/// A lane symbol (Dancing☆Onigiri's non-arrow lanes), drawn by
/// `shader.wgsl` from `SYMBOL_BASE` on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Symbol {
    Star = 16,
    Square = 17,
    Dot = 18,
}

/// How a [`Symbol`] is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SymbolMode {
    /// A thick stroke along the outline with a darker rim, as the arrows
    /// are drawn.
    Fill = 0,
    /// A ring along the outline (receptors).
    Outline = 1,
    /// Rings flowing outward from `color` to `params[0..3]`, shifted by
    /// `params[3]` cycles.
    Flow = 2,
    /// `params[0..3]` at the centre to `color` at the rim.
    Ramp = 3,
}

impl Symbol {
    /// The instance shape id of this symbol drawn in `mode`.
    pub fn shape(self, mode: SymbolMode) -> u32 {
        self as u32 | (mode as u32) << 8
    }
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
    /// 1 when the target is sRGB: the shader's own colour maths then works
    /// in linear light.
    srgb: f32,
    _pad: [f32; 2],
}

pub struct SpritePipeline {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
    srgb: bool,
}

impl SpritePipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> SpritePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-sprite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-globals-bgl"),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-globals-bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-sprite-layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ddi-sprite-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(Instance::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
        });
        let capacity = 1024;
        let instances = Self::make_buffer(device, capacity);
        SpritePipeline {
            pipeline,
            globals,
            bind_group,
            instances,
            capacity,
            srgb: format.is_srgb(),
        }
    }

    fn make_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-instances"),
            size: (capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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

    /// Upload instances; grows the buffer when needed.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, instances: &[Instance]) {
        if instances.len() > self.capacity {
            self.capacity = instances.len().next_power_of_two();
            self.instances = Self::make_buffer(device, self.capacity);
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
        }
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, count: usize) {
        if count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, 0..count as u32);
    }
}

#[cfg(test)]
mod tests {
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
