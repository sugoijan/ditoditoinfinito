//! Depth-buffered, simply lit character texture shared by both shells.
//! Deformation is sampled by `ddi-character`; this pass needs no compute/storage buffers.
#[path = "character_backing.rs"]
mod backing;
#[path = "character_tiles.rs"]
mod tiles;
use bytemuck::{Pod, Zeroable};
use ddi_character::appearance::{Appearance, tile_color};
use ddi_character::math::{Mat4, mul, normalize};
use ddi_character::{Character, Pose, SurfaceKind};
use wgpu::util::DeviceExt;
pub const SOURCE: &str = "dancing-hand-preview";
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Framing in hand-sized units; leaves model/contact coordinates unchanged.
    pub scale: f32,
    pub floor: bool,
    /// Optional normalized screen Y for the lowest mat corner (presentation only).
    pub ground_y: Option<f32>,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.,
            pitch: 5.,
            distance: 0.52,
            scale: 1.,
            floor: true,
            ground_y: None,
        }
    }
}
/// Render state independent from asset deformation.
pub struct Presentation {
    pub camera: Camera,
    pub size: [u32; 2],
    pub appearance: Option<Appearance>,
    pub tiles: [f32; 9],
    pub strikes: [u32; 9],
    pub seconds: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    detail: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    vp: Mat4,
    light: [f32; 4],
    tips: [[f32; 4]; 2],
    eye: [f32; 4],
    tint: [f32; 4],
    lighting: [f32; 4],
    surface: [f32; 4],
    mat_color: [f32; 4],
    accent: [f32; 4],
    tiles: [[f32; 4]; 3],
    tile_heights: [[f32; 4]; 3],
    tile_colors: [[f32; 4]; 9],
    effects: [f32; 4],
    capsules_a: [[f32; 4]; 6],
    capsules_b: [[f32; 4]; 6],
}
struct Target {
    color: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
    size: [u32; 2],
}
pub struct CharacterScene {
    device: wgpu::Device,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    floor: wgpu::Buffer,
    tiles: wgpu::Buffer,
    tile_vertices: u32,
    materials: Vec<wgpu::BindGroup>,
    hand: wgpu::RenderPipeline,
    cull: wgpu::RenderPipeline,
    floor_pipeline: wgpu::RenderPipeline,
    tile_pipeline: wgpu::RenderPipeline,
    target: Option<Target>,
    backing: Option<backing::Backing>,
    backing_active: bool,
    upload: Vec<Vertex>,
    occluder_joints: [[Option<usize>; 3]; 2],
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
pub fn animated_camera(mut camera: Camera, appearance: Option<Appearance>, seconds: f32) -> Camera {
    if let Some(a) = appearance {
        camera.yaw += a.sway * 12. * (seconds * 0.17).sin();
        camera.pitch += a.sway * 4. * (seconds * 0.13).sin();
        camera.floor = a.mat;
        if a.has_backing() {
            camera.ground_y = camera
                .ground_y
                .map(|y| y.min(0.98 - a.backing_margin.clamp(0., 0.06)));
        }
    }
    camera
}
/// Normalized viewport location for UI-only framing helpers. Never rendered here.
pub fn project(camera: Camera, aspect: f32, point: [f32; 3]) -> Option<[f32; 2]> {
    let m = view_projection(camera, aspect);
    let v: [f32; 4] = std::array::from_fn(|r| {
        m[r] * point[0] + m[4 + r] * point[1] + m[8 + r] * point[2] + m[12 + r]
    });
    if v[3] <= 0. {
        return None;
    }
    Some([v[0] / v[3] * 0.5 + 0.5, 0.5 - v[1] / v[3] * 0.5])
}
pub fn view_projection(camera: Camera, aspect: f32) -> Mat4 {
    let yaw = camera.yaw.to_radians();
    let pitch = camera.pitch.clamp(-75., 80.).to_radians();
    let dist = camera.distance.clamp(0.2, 1.2) * camera.scale * (1. / aspect.max(0.2)).max(1.);
    let target = [0., 0.105 * camera.scale, 0.];
    let eye = [
        dist * yaw.sin() * pitch.cos(),
        target[1] + dist * pitch.sin(),
        dist * yaw.cos() * pitch.cos(),
    ];
    let back = normalize(std::array::from_fn(|i| eye[i] - target[i]));
    let right = normalize(cross([0., 1., 0.], back));
    let up = cross(back, right);
    let view = [
        right[0],
        up[0],
        back[0],
        0.,
        right[1],
        up[1],
        back[1],
        0.,
        right[2],
        up[2],
        back[2],
        0.,
        -dot(right, eye),
        -dot(up, eye),
        -dot(back, eye),
        1.,
    ];
    let f = 1. / (15_f32.to_radians().tan());
    let near = 0.005 * camera.scale;
    let far = 3. * camera.scale;
    let projection = [
        f / aspect.max(0.01),
        0.,
        0.,
        0.,
        0.,
        f,
        0.,
        0.,
        0.,
        0.,
        far / (near - far),
        -1.,
        0.,
        0.,
        near * far / (near - far),
        0.,
    ];
    let mut vp = mul(&projection, &view);
    if let Some(y) = camera.ground_y.filter(|y| y.is_finite()) {
        let mut bottom = f32::NEG_INFINITY;
        for x in [-0.036, 0.036] {
            for z in [-0.036, 0.036] {
                let point = [x * camera.scale, 0., z * camera.scale, 1.];
                let clip_y = (0..4).map(|i| vp[i * 4 + 1] * point[i]).sum::<f32>();
                let w = (0..4).map(|i| vp[i * 4 + 3] * point[i]).sum::<f32>();
                if w > 0. {
                    bottom = bottom.max(0.5 - clip_y / w * 0.5);
                }
            }
        }
        if bottom.is_finite() {
            let shift = -2. * (y.clamp(0., 1.) - bottom);
            for i in 0..4 {
                vp[i * 4 + 1] += shift * vp[i * 4 + 3];
            }
        }
    }
    vp
}
impl CharacterScene {
    pub fn new(device: &wgpu::Device, asset: &Character) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("character-camera"),
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
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("character-camera"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("character-camera"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let ml = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("character-material"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let materials = asset
            .primitives
            .iter()
            .map(|p| {
                let b = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("character-color"),
                    contents: bytemuck::cast_slice(&[
                        p.color,
                        [p.surface as u8 as f32, 0., 0., 0.],
                    ]),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("character-color"),
                    layout: &ml,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: b.as_entire_binding(),
                    }],
                })
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("character"),
            source: wgpu::ShaderSource::Wgsl(include_str!("character.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("character"),
            bind_group_layouts: &[Some(&layout), Some(&ml)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, cull, vertex| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 40,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x4],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24Plus,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let hand = pipeline("fs_hand", None, "vs_main");
        let cull = pipeline("fs_hand", Some(wgpu::Face::Back), "vs_main");
        let floor_pipeline = pipeline("fs_floor", None, "vs_main");
        let tile_pipeline = pipeline("fs_floor", None, "vs_tiles");
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("character-vertices"),
            size: (asset.vertices.len() * 40) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("character-indices"),
            contents: bytemuck::cast_slice(&asset.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let floor: Vec<Vertex> = [
            [-0.18, 0., -0.18],
            [-0.18, 0., 0.18],
            [0.18, 0., 0.18],
            [-0.18, 0., -0.18],
            [0.18, 0., 0.18],
            [0.18, 0., -0.18],
        ]
        .into_iter()
        .map(|position| Vertex {
            position,
            normal: [0., 1., 0.],
            detail: [0.; 4],
        })
        .collect();
        let floor = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("character-floor"),
            contents: bytemuck::cast_slice(&floor),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let tile_data = tiles::vertices();
        let tile_vertices = tile_data.len() as u32;
        let tiles = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("character-buttons"),
            contents: bytemuck::cast_slice(&tile_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Self {
            device: device.clone(),
            uniform,
            group,
            vertices,
            indices,
            floor,
            tiles,
            tile_vertices,
            materials,
            hand,
            cull,
            floor_pipeline,
            tile_pipeline,
            target: None,
            backing: None,
            backing_active: false,
            upload: asset
                .surface_details()
                .into_iter()
                .map(|detail| Vertex {
                    position: [0.; 3],
                    normal: [0.; 3],
                    detail,
                })
                .collect(),
            occluder_joints: std::array::from_fn(|f| {
                std::array::from_fn(|j| {
                    let name = format!(
                        "{}.{}",
                        ["index", "middle"][f],
                        ["proximal", "middle", "distal"][j]
                    );
                    asset.joint_names.iter().position(|n| n == &name)
                })
            }),
        }
    }
    pub fn on(&self, device: &wgpu::Device) -> bool {
        self.device == *device
    }
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        if self.backing_active {
            self.backing.as_ref().and_then(|b| b.texture())
        } else {
            self.target.as_ref().map(|t| &t.color)
        }
    }
    /// Draws the pose for a presentation (camera, size, appearance).
    pub fn draw_styled(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        asset: &Character,
        pose: &Pose,
        presentation: Presentation,
    ) {
        let Presentation {
            mut camera,
            size,
            appearance,
            tiles,
            strikes,
            seconds,
        } = presentation;
        camera = animated_camera(camera, appearance, seconds);
        let limit = device.limits().max_texture_dimension_2d.min(2048) as f32;
        let scale = (limit / size[0].max(size[1]).max(1) as f32).min(1.);
        let size = size.map(|n| (n as f32 * scale).round().max(1.) as u32);
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let make = |format, usage, label| {
                crate::gpu::texture_2d(device, label, size, format, 1, usage, &[])
            };
            let color = make(
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                "character-color",
            );
            let depth = make(
                wgpu::TextureFormat::Depth24Plus,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
                "character-depth",
            );
            let view = color.create_view(&Default::default());
            self.target = Some(Target {
                color,
                view,
                depth: depth.create_view(&Default::default()),
                size,
            });
        }
        for (i, v) in self.upload.iter_mut().enumerate() {
            v.position = pose.positions[i];
            v.normal = pose.normals[i];
        }
        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&self.upload));
        let contacts = asset.contacts(pose);
        let tips = std::array::from_fn(|i| {
            let p = contacts.get(i).copied().unwrap_or([10., 10., 10.]);
            [p[0], p[2], p[1], 0.]
        });
        let a = appearance.unwrap_or_default();
        let yaw = camera.yaw.to_radians();
        let pitch = camera.pitch.clamp(-75., 80.).to_radians();
        let aspect = size[0] as f32 / size[1] as f32;
        let dist = camera.distance.clamp(0.2, 1.2) * camera.scale * (1. / aspect.max(0.2)).max(1.);
        let phase = seconds * a.bpm / 60.;
        let pulse = 1. + a.pulse * 0.25 * (std::f32::consts::TAU * phase).cos().max(0.);
        let angle = a.light_yaw.to_radians();
        let mut capsules_a = [[0., -10., 0., 0.]; 6];
        let mut capsules_b = capsules_a;
        if appearance.is_some_and(|a| a.studio) {
            for finger in 0..2 {
                if let [Some(a), Some(b), Some(c)] = self.occluder_joints[finger] {
                    let points = [a, b, c].map(|id| {
                        [
                            pose.worlds[id][12],
                            pose.worlds[id][13],
                            pose.worlds[id][14],
                        ]
                    });
                    let tip = contacts.get(finger).copied().unwrap_or(points[2]);
                    let points = [points[0], points[1], points[2], tip];
                    for j in 0..3 {
                        let i = finger * 3 + j;
                        let r = [0.0035, 0.003, 0.0025][j];
                        capsules_a[i] = [points[j][0], points[j][1], points[j][2], r];
                        capsules_b[i] = [points[j + 1][0], points[j + 1][1], points[j + 1][2], r];
                    }
                }
            }
        }

        let heights = ddi_character::appearance::tile_heights(&contacts);
        let u = Uniform {
            vp: view_projection(camera, size[0] as f32 / size[1] as f32),
            light: if appearance.is_some() {
                [angle.sin(), 0.8, angle.cos(), 1.]
            } else {
                [-0.3, 0.8, 0.6, 0.]
            },
            eye: [
                dist * yaw.sin() * pitch.cos(),
                0.105 * camera.scale + dist * pitch.sin(),
                dist * yaw.cos() * pitch.cos(),
                0.,
            ],
            tint: [a.tint[0], a.tint[1], a.tint[2], 1.],
            lighting: [a.key * pulse, a.fill, a.rim * pulse, a.ambient],
            surface: [a.roughness, a.specular, a.response, f32::from(a.test_tile)],
            mat_color: [a.mat_color[0], a.mat_color[1], a.mat_color[2], 1.],
            accent: [a.accent[0], a.accent[1], a.accent[2], 1.],
            tile_colors: std::array::from_fn(|i| {
                let c = if a.random_colors {
                    tile_color(a.palette_seed, i, strikes[i])
                } else {
                    a.accent
                };
                [c[0], c[1], c[2], 1.]
            }),
            effects: [
                if a.studio { 1. } else { 0. },
                if a.mat { a.spill } else { 0. },
                if a.transparent { 1. } else { 0. },
                0.,
            ],
            capsules_a,
            capsules_b,
            tile_heights: std::array::from_fn(|r| {
                std::array::from_fn(|c| heights.get(r * 4 + c).copied().unwrap_or(0.))
            }),
            tiles: std::array::from_fn(|r| {
                std::array::from_fn(|c| tiles.get(r * 4 + c).copied().unwrap_or(0.))
            }),
            tips,
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        let t = self.target.as_ref().unwrap();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("character"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(if appearance.is_some_and(|a| a.transparent) {
                        wgpu::Color::TRANSPARENT
                    } else {
                        wgpu::Color {
                            r: 0.91,
                            g: 0.9,
                            b: 0.88,
                            a: 1.,
                        }
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_bind_group(1, &self.materials[0], &[]);
        if camera.floor {
            if appearance.is_some() {
                pass.set_pipeline(&self.tile_pipeline);
                pass.set_vertex_buffer(0, self.tiles.slice(..));
                pass.draw(0..self.tile_vertices, 0..1);
            } else {
                pass.set_pipeline(&self.floor_pipeline);
                pass.set_vertex_buffer(0, self.floor.slice(..));
                pass.draw(0..6, 0..1);
            }
        }
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for (i, p) in asset.primitives.iter().enumerate() {
            if appearance.is_some() && p.surface == SurfaceKind::DiagnosticMat {
                continue;
            }
            pass.set_pipeline(if p.double_sided {
                &self.hand
            } else {
                &self.cull
            });
            pass.set_bind_group(1, &self.materials[i], &[]);
            pass.draw_indexed(p.indices.clone(), 0, 0..1);
        }
        drop(pass);
        self.backing_active = appearance.is_some_and(Appearance::has_backing);
        if let Some(look) = appearance.filter(|a| a.has_backing()) {
            let backing = self
                .backing
                .get_or_insert_with(|| backing::Backing::new(device));
            backing.draw(device, queue, encoder, &t.view, size, look);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn up_guide_uses_exported_panel_eight_direction() {
        let camera = super::Camera {
            pitch: 30.,
            ..Default::default()
        };
        let up = super::project(camera, 1., ddi_character::appearance::UP_TILE_CENTER).unwrap();
        let down = super::project(camera, 1., [0., 0., -0.024]).unwrap();
        assert!((up[0] - 0.5).abs() < 1e-6);
        assert!(up[1] > down[1]);
    }
    #[test]
    fn character_uniform_layout_and_webgl2_shader() {
        let module = naga::front::wgsl::parse_str(include_str!("character.wgsl")).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let (_, camera) = module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref() == Some("Camera"))
            .unwrap();
        let naga::TypeInner::Struct { span, .. } = camera.inner else {
            panic!("camera is not a struct")
        };
        assert_eq!(span as usize, std::mem::size_of::<super::Uniform>());
        for (stage, entry) in [
            (naga::ShaderStage::Vertex, "vs_main"),
            (naga::ShaderStage::Vertex, "vs_tiles"),
            (naga::ShaderStage::Fragment, "fs_hand"),
            (naga::ShaderStage::Fragment, "fs_floor"),
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

#[cfg(test)]
mod ground_framing_tests {
    use super::*;
    #[test]
    fn mat_bottom_stays_anchored_through_distance_orbit_and_aspect_changes() {
        for aspect in [0.45, 0.9, 1.6] {
            for yaw in [-180., -90., 19., 90., 180.] {
                for distance in [0.25, 0.43, 0.9] {
                    let camera = Camera {
                        yaw,
                        pitch: 21.,
                        distance,
                        ground_y: Some(0.98),
                        ..Default::default()
                    };
                    let bottom = [
                        [-0.036, 0., -0.036],
                        [-0.036, 0., 0.036],
                        [0.036, 0., -0.036],
                        [0.036, 0., 0.036],
                    ]
                    .map(|p| project(camera, aspect, p).unwrap()[1])
                    .into_iter()
                    .fold(f32::NEG_INFINITY, f32::max);
                    assert!(
                        (bottom - 0.98).abs() < 1e-5,
                        "{aspect}/{yaw}/{distance}: {bottom}"
                    );
                }
            }
        }
    }
}
