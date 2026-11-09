//! The wgpu boilerplate the pipelines share.

/// Fills `texture`'s first level (`size` pixels) with RGBA rows,
/// 4 bytes a pixel and tightly packed.
pub fn write_rgba(queue: &wgpu::Queue, texture: &wgpu::Texture, [w, h]: [u32; 2], rgba: &[u8]) {
    queue.write_texture(
        texture.as_image_copy(),
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}

/// A pipeline of triangles from the shader's `vs_main`, through
/// `fragment` into one color target.
pub(crate) struct QuadPipeline<'a> {
    pub(crate) label: &'a str,
    pub(crate) layout: &'a wgpu::PipelineLayout,
    pub(crate) shader: &'a wgpu::ShaderModule,
    pub(crate) fragment: &'a str,
    pub(crate) buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) blend: Option<wgpu::BlendState>,
    pub(crate) write_mask: wgpu::ColorWrites,
    pub(crate) depth_stencil: Option<wgpu::DepthStencilState>,
}

impl QuadPipeline<'_> {
    pub(crate) fn make(self, device: &wgpu::Device) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(self.label),
            layout: Some(self.layout),
            vertex: wgpu::VertexState {
                module: self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: self.buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: self.shader,
                entry_point: Some(self.fragment),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: self.blend,
                    write_mask: self.write_mask,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: self.depth_stencil,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }
}

/// A pipeline drawing one triangle over the target, with no vertex
/// buffers (the shader's `vs_main`), through `fragment` into `format`.
pub(crate) fn fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fragment: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    QuadPipeline {
        label,
        layout,
        shader,
        fragment,
        buffers: &[],
        format,
        blend,
        write_mask: wgpu::ColorWrites::ALL,
        depth_stencil: None,
    }
    .make(device)
}

/// A pass drawing to `target` (`load`ed, and stored), with `depth` (the
/// stencil) if given.
pub(crate) fn color_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
    depth: Option<wgpu::RenderPassDepthStencilAttachment<'_>>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: depth,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// A uniform buffer at `binding`, seen by `visibility`, with a dynamic
/// offset when `dynamic`.
pub(crate) const fn uniform_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    dynamic: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

/// A 2D texture at `binding`, for the fragment stage.
pub(crate) const fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// A sampler at `binding`, for the fragment stage.
pub(crate) const fn sampler_entry(binding: u32, filtering: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(if filtering {
            wgpu::SamplerBindingType::Filtering
        } else {
            wgpu::SamplerBindingType::NonFiltering
        }),
        count: None,
    }
}

/// A sampler filtering linearly, clamped to the edges, without mipmaps.
pub(crate) fn linear_sampler(device: &wgpu::Device, label: &str) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

/// A uniform buffer of `size` bytes the queue writes (labeled `label`), a
/// layout of it at binding 0, seen by `visibility`, and a group binding it
/// there (labeled `<label>-bgl` and `<label>-bg`).
pub(crate) fn uniform_group(
    device: &wgpu::Device,
    label: &str,
    size: usize,
    visibility: wgpu::ShaderStages,
) -> (wgpu::Buffer, wgpu::BindGroupLayout, wgpu::BindGroup) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(&format!("{label}-bgl")),
        entries: &[uniform_entry(0, visibility, false)],
    });
    let (buffer, group) = uniform_group_in(device, label, size, &layout);
    (buffer, layout, group)
}

/// A uniform buffer of `size` bytes the queue writes (labeled `label`) and
/// a group binding it at binding 0 of `layout` (labeled `<label>-bg`).
pub(crate) fn uniform_group_in(
    device: &wgpu::Device,
    label: &str,
    size: usize,
    layout: &wgpu::BindGroupLayout,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("{label}-bg")),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (buffer, group)
}

/// A layout of a filterable 2D texture at binding 0 and a filtering
/// sampler at binding 1, both for the fragment stage.
pub(crate) fn texture_sampler_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[texture_entry(0, true), sampler_entry(1, true)],
    })
}

/// A layout of a uniform at binding 0, a filterable 2D texture at binding 1
/// and a filtering sampler at binding 2, all for the fragment stage, as
/// [`uniform_texture_group`] binds them.
pub(crate) fn uniform_texture_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::FRAGMENT, false),
            texture_entry(1, true),
            sampler_entry(2, true),
        ],
    })
}

/// A 2D texture of one sample a texel, `width × height` as given.
pub(crate) fn texture_2d(
    device: &wgpu::Device,
    label: &str,
    [width, height]: [u32; 2],
    format: wgpu::TextureFormat,
    mip_level_count: u32,
    usage: wgpu::TextureUsages,
    view_formats: &[wgpu::TextureFormat],
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats,
    })
}

/// A group of `view` at binding 0 and `sampler` at binding 1, as
/// [`texture_sampler_layout`] lays them out.
pub(crate) fn texture_sampler_group(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
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
}

/// A group of `uniform` at binding 0, `view` at binding 1 and `sampler`
/// at binding 2.
pub(crate) fn uniform_texture_group(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    uniform: wgpu::BindingResource<'_>,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform,
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// The format of pictures drawn for the 2D pass to show: encoded values,
/// as its images.
pub(crate) const PICTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A picture drawn offscreen for the 2D pass to show (the field
/// preview, the editor's field, a preview movie): its texture, made again
/// at a new size under a new generation (the painter's image set again
/// then).
#[derive(Default)]
pub(crate) struct PictureTarget {
    target: Option<(wgpu::Texture, wgpu::TextureView)>,
    generation: u64,
}

impl PictureTarget {
    /// The texture at `size` (`format`), made now when it is not.
    pub(crate) fn ensure(
        &mut self,
        device: &wgpu::Device,
        label: &str,
        size: [u32; 2],
        format: wgpu::TextureFormat,
    ) {
        if self
            .target
            .as_ref()
            .is_some_and(|(t, _)| [t.width(), t.height()] == size)
        {
            return;
        }
        let texture = texture_2d(
            device,
            label,
            size,
            format,
            1,
            // Read back by tests.
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            &[],
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.target = Some((texture, view));
        self.generation += 1;
    }

    pub(crate) fn texture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|(t, _)| t)
    }

    pub(crate) fn view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref().map(|(_, v)| v)
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn release(&mut self) {
        self.target = None;
    }

    pub(crate) fn is_some(&self) -> bool {
        self.target.is_some()
    }
}
