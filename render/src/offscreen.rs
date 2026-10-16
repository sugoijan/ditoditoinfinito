//! A frame drawn offscreen, for targets with no plain view (WebGL2's
//! surface is sRGB and nothing else; `docs/plans/bundled-skins.md`, step
//! 5): the scene in a float texture of linear light (blending there is in
//! linear light, as through an sRGB view), each run of a skin's quads in a
//! plain texture holding the same frame as encoded values (blending there
//! is StepMania's), then the frame drawn to the target
//! (`Renderer::render`). The conversions are the shader's own and no sRGB
//! texture is read on the way: Firefox reads one wrongly while drawing to
//! a plain target, and copies between the two are refused (Chrome) or
//! converted (Firefox).

/// The textures of one frame size.
struct Targets {
    size: (u32, u32),
    scene: wgpu::TextureView,
    plain: wgpu::TextureView,
    /// Sampling the scene, the plain texture.
    scene_group: wgpu::BindGroup,
    plain_group: wgpu::BindGroup,
}

pub(crate) struct Offscreen {
    scene_format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// The scene into the plain texture, back, and to the target.
    encode: wgpu::RenderPipeline,
    decode: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
    targets: Option<Targets>,
}

/// The plain texture's format: encoded 8-bit values, as skins' textures.
pub(crate) const PLAIN: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Offscreen {
    /// For a `target` (an sRGB format), the scene drawn in `scene_format`
    /// (a float format the device renders and blends).
    pub(crate) fn new(
        device: &wgpu::Device,
        target: wgpu::TextureFormat,
        scene_format: wgpu::TextureFormat,
    ) -> Offscreen {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-offscreen"),
            source: wgpu::ShaderSource::Wgsl(include_str!("offscreen.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-offscreen-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ddi-offscreen-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ddi-offscreen-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Offscreen {
            scene_format,
            encode: pipeline("fs_encode", PLAIN),
            decode: pipeline("fs_decode", scene_format),
            // Linear light into the sRGB target, which encodes it.
            present: pipeline("fs_copy", target),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ddi-offscreen-sampler"),
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }),
            layout,
            targets: None,
        }
    }

    /// Lets the frame's textures go (made again by the next frame).
    pub(crate) fn release_targets(&mut self) {
        self.targets = None;
    }

    /// Makes the textures for a `width × height` frame (again, when the
    /// size changed).
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let size = (width.max(1), height.max(1));
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |format, label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let scene = texture(self.scene_format, "ddi-offscreen-scene");
        let plain = texture(PLAIN, "ddi-offscreen-plain");
        let group = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ddi-offscreen-bg"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        self.targets = Some(Targets {
            size,
            scene_group: group(&scene),
            plain_group: group(&plain),
            scene,
            plain,
        });
    }

    fn targets(&self) -> &Targets {
        self.targets
            .as_ref()
            .expect("Offscreen::prepare comes first")
    }

    /// The scene's texture.
    pub(crate) fn scene(&self) -> &wgpu::TextureView {
        &self.targets().scene
    }

    /// The plain texture.
    pub(crate) fn plain(&self) -> &wgpu::TextureView {
        &self.targets().plain
    }

    /// In a pass on the plain texture: the scene so far, encoded.
    pub(crate) fn scene_to_plain(&self, pass: &mut wgpu::RenderPass<'_>) {
        draw(pass, &self.encode, &self.targets().scene_group);
    }

    /// In a pass on the scene's texture: the plain texture, decoded.
    pub(crate) fn plain_to_scene(&self, pass: &mut wgpu::RenderPass<'_>) {
        draw(pass, &self.decode, &self.targets().plain_group);
    }

    /// In a pass on the target: the finished scene.
    pub(crate) fn scene_to_target(&self, pass: &mut wgpu::RenderPass<'_>) {
        draw(pass, &self.present, &self.targets().scene_group);
    }
}

fn draw(pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup) {
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    #[test]
    fn shader_validates() {
        let source = include_str!("offscreen.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("WGSL validates");
    }

    /// The shader's conversions, in f32 as a GPU does them, the scene kept
    /// in half floats: every 8-bit value comes back as itself from the
    /// plain texture to the scene and back.
    #[test]
    fn eight_bit_values_survive_the_round_trip() {
        let encode = |c: f32| {
            if c <= 0.0031308 {
                c * 12.92
            } else {
                1.055 * c.max(0.0).powf(1.0 / 2.4) - 0.055
            }
        };
        let decode = |c: f32| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        // Half-float storage: 11 significant bits.
        let half = |x: f32| {
            if x == 0.0 {
                return 0.0;
            }
            let step = 2f32.powf(x.abs().log2().floor() - 10.0);
            (x / step).round() * step
        };
        for v in 0..=255u8 {
            let scene = half(decode(f32::from(v) / 255.0));
            assert_eq!((encode(scene) * 255.0).round(), f32::from(v), "{v}");
        }
    }
}
