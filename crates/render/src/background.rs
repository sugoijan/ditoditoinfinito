//! Background images behind the field: the song's background and the
//! images and movies its background changes show, crossfaded.
//!
//! The renderer never decodes images: the shell creates each texture with
//! [`BackgroundPipeline::create_texture`] and fills it (the web shell copies
//! an `ImageBitmap` the browser decoded; a desktop shell would write RGBA
//! bytes), then registers it with [`BackgroundPipeline::add_texture`] and
//! picks what to show each frame with a [`Backdrop`]. A movie is registered
//! once with [`BackgroundPipeline::add_video`] and given each frame's YUV
//! planes with [`BackgroundPipeline::set_video_frame`]; the shader converts
//! them, so images and movies crossfade alike.
//!
//! Layers are cover-fitted into a frame, a centred rectangle of a chosen
//! shape (the screen the pack's art was made for). A full-screen Gaussian
//! base follows those same layers, even with a screen-sized frame. Static
//! sources reuse their filtered copies; video uploads and changed layer
//! weights refresh them. The entrance temporarily adds softly blurred
//! content, continuing the prompt's visual progress (`ddi_actor::background`).

use bytemuck::{Pod, Zeroable};

/// Draws a frame can make: a black base under the first layer, the first
/// layer's two, then the second's.
const DRAWS: usize = 5;
type FilterKey = (u64, [Option<(usize, f32)>; DRAWS]);

/// The id a [`Backdrop`] shows the plain backdrop by: the color given to
/// [`BackgroundPipeline::set_plain_color`] (the field's), opaque.
pub const PLAIN: usize = usize::MAX;
/// The black base under the first layer.
const BLACK: usize = usize::MAX - 1;
/// Uniform slots per image: sharp content, ambient base and soft content,
/// for each place it may be drawn in (an image can show on both layers).
const SLOTS: u64 = 3 * DRAWS as u64;
/// `blur.wgsl`'s `Blur` uniform.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct BlurParams {
    pub(crate) step: [f32; 2],
    pub(crate) brightness: f32,
    /// 1: the target holds linear light (the brightness is applied to the
    /// encoded values all the same).
    pub(crate) linear: f32,
    /// The field filter's band, as [`Params`] has it.
    pub(crate) band: [f32; 2],
    pub(crate) band_keep: f32,
    /// 1: return straight alpha (a cached image's vertical pass).
    pub(crate) unpremultiply: f32,
    pub(crate) uv_scale: [f32; 2],
    pub(crate) uv_offset: [f32; 2],
    pub(crate) opacity: f32,
    pub(crate) base: f32,
    pub(crate) _pad2: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Params {
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    brightness: f32,
    alpha: f32,
    kr: f32,
    kb: f32,
    full_range: f32,
    linear_out: f32,
    /// The field filter's band: its left and right edges (target pixels)
    /// and the brightness kept inside it (1: no band).
    band: [f32; 2],
    band_keep: f32,
    source_linear: f32,
    _pad: [f32; 2],
}

/// What to show behind the field in one frame. While `mix < 1` a crossfade
/// is running from `previous` to `current` (`mix` = how far, 1 = only
/// `current`). Ids are from [`BackgroundPipeline::add_texture`]; `None` is
/// no image, the plain field color. `second` is drawn over them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Backdrop {
    pub current: Option<usize>,
    pub previous: Option<usize>,
    pub mix: f32,
    /// The second background layer (`#BGCHANGES2`).
    pub second: SecondLayer,
}

/// The second background layer, drawn over the first as StepMania draws a
/// layer: `current` fully shown, and while `mix < 1` `previous` over it,
/// fading out (`BackgroundImpl::DrawPrimitives`; a transition fades the
/// background that leaves). Images keep their own transparency, so the
/// first layer shows through where they have none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SecondLayer {
    pub current: Option<usize>,
    pub previous: Option<usize>,
    pub mix: f32,
}

impl SecondLayer {
    /// The draws, back to front: `(id, opacity)`.
    pub fn layers(&self) -> [Option<(usize, f32)>; 2] {
        let mix = self.mix.clamp(0.0, 1.0);
        let current = self.current.map(|c| (c, 1.0));
        match self.previous {
            Some(p) if mix < 1.0 => [current, Some((p, 1.0 - mix))],
            _ => [current, None],
        }
    }
}

impl Backdrop {
    /// One image, fully shown.
    pub fn image(id: usize) -> Backdrop {
        Backdrop {
            current: Some(id),
            previous: None,
            mix: 1.0,
            second: SecondLayer::default(),
        }
    }

    /// The first layer's draws, back to front: `(id, opacity)`. As
    /// StepMania draws a layer: the incoming one, then the outgoing one
    /// over it fading out (`BackgroundImpl::DrawPrimitives`,
    /// `BackgroundTransitions/CrossFade.xml`), over black (no image
    /// either way: the black base shows).
    pub fn layers(&self) -> [Option<(usize, f32)>; 2] {
        let mix = self.mix.clamp(0.0, 1.0);
        if mix >= 1.0 || self.previous == self.current {
            return [self.current.map(|c| (c, 1.0)), None];
        }
        match (self.previous, self.current) {
            (Some(p), Some(c)) => [Some((c, 1.0)), Some((p, 1.0 - mix))],
            (None, Some(c)) => [Some((c, mix)), None],
            (Some(p), None) => [Some((p, 1.0 - mix)), None],
            (None, None) => [None, None],
        }
    }

    /// Every draw, back to front: black under the first layer when it
    /// draws (StepMania's screen is black under its backgrounds, so a
    /// transparent image shows black), the first layer's, the second's.
    pub fn draws(&self) -> [Option<(usize, f32)>; DRAWS] {
        let [a, b] = self.layers();
        let [c, d] = self.second.layers();
        let base = (a.is_some() || b.is_some()).then_some((BLACK, 1.0));
        [base, a, b, c, d]
    }
}

/// One registered image or movie. Its parameters have one slot per draw
/// ([`SLOTS`]), picked by a dynamic offset.
struct Image {
    group: wgpu::BindGroup,
    params: wgpu::Buffer,
    width: f32,
    height: f32,
    /// A movie: its planes and the current frame's color tags.
    video: Option<Video>,
}

/// One-pixel images of a color: the black base and the plain backdrop.
struct Solids {
    black: Image,
    plain: Image,
    plain_texture: wgpu::Texture,
    /// The color written into `plain_texture`.
    plain_written: [u8; 4],
}

struct Video {
    /// Y, U, V.
    planes: [wgpu::Texture; 3],
    /// BT.709 (else BT.601).
    bt709: bool,
    full_range: bool,
    nv12: bool,
}

/// The YUV matrix's luma weights of red and blue.
fn weights(bt709: bool) -> (f32, f32) {
    if bt709 {
        (0.2126, 0.0722)
    } else {
        (0.299, 0.114)
    }
}

/// Texture coordinates `(scale, offset)` that cover a `target_w × target_h`
/// area with an `image_w × image_h` image, cropping the overflow evenly.
pub fn cover_uv(target_w: f32, target_h: f32, image_w: f32, image_h: f32) -> ([f32; 2], [f32; 2]) {
    let [x, y] = ddi_actor::background::cover_scale(target_w, target_h, image_w, image_h);
    ([x, y], [(1.0 - x) / 2.0, (1.0 - y) / 2.0])
}

/// Texture coordinates of the part `(scale, offset)` of the frame (in the
/// frame's own `0..1` coordinates) within an image shown in the frame at
/// `frame`'s coordinates.
#[cfg(test)]
fn extension_uv(
    (frame_scale, frame_offset): ([f32; 2], [f32; 2]),
    (part_scale, part_offset): ([f32; 2], [f32; 2]),
) -> ([f32; 2], [f32; 2]) {
    (
        [
            frame_scale[0] * part_scale[0],
            frame_scale[1] * part_scale[1],
        ],
        [
            frame_offset[0] + frame_scale[0] * part_offset[0],
            frame_offset[1] + frame_scale[1] * part_offset[1],
        ],
    )
}

/// The frame for a `width × height` target and a frame shape (width over
/// height): `(x, y, width, height)` in pixels, centred and as large as fits;
/// `None` for the whole target (no shape, or one within a pixel of it).
pub fn frame_rect(width: f32, height: f32, aspect: Option<f32>) -> Option<[f32; 4]> {
    let aspect = aspect.filter(|a| a.is_finite() && *a > 0.0)?;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let (w, h) = if width / height > aspect {
        (height * aspect, height)
    } else {
        (width, width / aspect)
    };
    (width - w >= 1.0 || height - h >= 1.0).then(|| [(width - w) / 2.0, (height - h) / 2.0, w, h])
}

/// Multiplier for a perceived brightness in `0..=1`, applied to the
/// encoded values on either target (on an sRGB one the shaders encode,
/// multiply and decode: the same pixels as a plain target's, and as the
/// 2D pass's tint draws the start prompt's picture).
pub fn brightness_factor(perceived: f32) -> f32 {
    perceived.clamp(0.0, 1.0)
}

/// What [`BackgroundPipeline::prepare`] set up for one frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Prepared {
    /// `(id, opacity)` of each draw ([`Backdrop::draws`]).
    layers: [Option<(usize, f32)>; DRAWS],
    /// The content rectangle, including during its entrance.
    frame: Option<[f32; 4]>,
    target: (f32, f32),
    alphas: [f32; 3],
    dirty: [bool; 2],
    key: Option<FilterKey>,
}

/// A filtered source: compose into `a`, blur through `b` back into `a`.
/// Groups: horizontal, vertical, base, strong content, soft content.
struct Wide {
    size: (u32, u32),
    a: wgpu::TextureView,
    b: wgpu::TextureView,
    groups: [wgpu::BindGroup; 5],
    params: [wgpu::Buffer; 5],
}

pub struct BackgroundPipeline {
    pipeline: wgpu::RenderPipeline,
    base_pipeline: wgpu::RenderPipeline,
    base_yuv_pipeline: wgpu::RenderPipeline,
    #[cfg(feature = "video-planes")]
    base_nv12_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    yuv_pipeline: wgpu::RenderPipeline,
    #[cfg(feature = "video-planes")]
    nv12_pipeline: wgpu::RenderPipeline,
    yuv_layout: wgpu::BindGroupLayout,
    blur_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    blur_layout: wgpu::BindGroupLayout,
    /// The pipelines' layouts: of `layout`, `yuv_layout` and `blur_layout`.
    pipeline_layouts: [wgpu::PipelineLayout; 3],
    shader: wgpu::ShaderModule,
    blur_shader: wgpu::ShaderModule,
    sampler: wgpu::Sampler,
    images: Vec<Image>,
    /// The black base and the plain backdrop, made when first drawn.
    solids: Option<Solids>,
    /// The plain backdrop's color (sRGB-encoded RGBA).
    plain: [u8; 4],
    wide: Option<Wide>,
    soft: Option<Wide>,
    base_size: Option<[f32; 2]>,
    revision: u64,
    filtered: [Option<FilterKey>; 2],
    filter_jobs: [u64; 2],
    format: wgpu::TextureFormat,
    /// Bytes between an image's parameter slots.
    stride: u64,
    /// The target holds linear light (sRGB, or a float stand-in).
    srgb: bool,
}

impl BackgroundPipeline {
    /// Backgrounds drawn to `format`, whose values are linear light when
    /// `linear` (an sRGB target, or a float one standing in for it).
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        linear: bool,
    ) -> BackgroundPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-background"),
            source: wgpu::ShaderSource::Wgsl(include_str!("background.wgsl").into()),
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let uniform = |dynamic| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        };
        let common = [
            uniform(true),
            texture_entry(1),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-background-bgl"),
            entries: &common,
        });
        let yuv_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-background-yuv-bgl"),
            entries: &[
                common[0],
                common[1],
                common[2],
                texture_entry(3),
                texture_entry(4),
            ],
        });
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ddi-background-blur"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ddi-background-blur-bgl"),
            entries: &[uniform(false), common[1], common[2]],
        });
        let pipeline_layouts = [&layout, &yuv_layout, &blur_layout].map(|layout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ddi-background-layout"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            })
        });
        let [pipeline_layout, yuv_pipeline_layout, _] = &pipeline_layouts;
        let [pipeline, yuv_pipeline, blur_pipeline, composite_pipeline] =
            Self::pipelines(device, format, &pipeline_layouts, [&shader, &blur_shader]);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ddi-background-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let stride = u64::from(device.limits().min_uniform_buffer_offset_alignment)
            .max(std::mem::size_of::<Params>() as u64);
        BackgroundPipeline {
            base_pipeline: Self::pipeline_for(
                device,
                wgpu::TextureFormat::Rgba8Unorm,
                pipeline_layout,
                &shader,
                "fs_main",
                Some(wgpu::BlendState::ALPHA_BLENDING),
            ),
            base_yuv_pipeline: Self::pipeline_for(
                device,
                wgpu::TextureFormat::Rgba8Unorm,
                yuv_pipeline_layout,
                &shader,
                "fs_yuv",
                Some(wgpu::BlendState::ALPHA_BLENDING),
            ),
            #[cfg(feature = "video-planes")]
            base_nv12_pipeline: Self::pipeline_for(
                device,
                wgpu::TextureFormat::Rgba8Unorm,
                yuv_pipeline_layout,
                &shader,
                "fs_nv12",
                Some(wgpu::BlendState::ALPHA_BLENDING),
            ),
            pipeline,
            layout,
            yuv_pipeline,
            #[cfg(feature = "video-planes")]
            nv12_pipeline: Self::pipeline_for(
                device,
                format,
                yuv_pipeline_layout,
                &shader,
                "fs_nv12",
                Some(wgpu::BlendState::ALPHA_BLENDING),
            ),
            yuv_layout,
            blur_pipeline,
            composite_pipeline,
            blur_layout,
            pipeline_layouts,
            shader,
            blur_shader,
            sampler,
            images: Vec::new(),
            solids: None,
            plain: [0, 0, 0, 255],
            wide: None,
            soft: None,
            base_size: None,
            revision: 0,
            filtered: [None, None],
            filter_jobs: [0; 2],
            format,
            stride,
            srgb: linear,
        }
    }

    /// Draws to `format` from now on (its values linear light as before);
    /// the images stay.
    pub fn retarget(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        [
            self.pipeline,
            self.yuv_pipeline,
            self.blur_pipeline,
            self.composite_pipeline,
        ] = Self::pipelines(
            device,
            format,
            &self.pipeline_layouts,
            [&self.shader, &self.blur_shader],
        );
        #[cfg(feature = "video-planes")]
        {
            self.nv12_pipeline = Self::pipeline_for(
                device,
                format,
                &self.pipeline_layouts[1],
                &self.shader,
                "fs_nv12",
                Some(wgpu::BlendState::ALPHA_BLENDING),
            );
        }
        self.format = format;
        // Composite pipelines changed; regenerate their bind groups.
        self.wide = None;
        self.soft = None;
        self.filtered = [None, None];
    }

    /// The image, movie frame, blur and composite pipelines.
    fn pipelines(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        [layout, yuv_layout, blur_layout]: &[wgpu::PipelineLayout; 3],
        [shader, blur_shader]: [&wgpu::ShaderModule; 2],
    ) -> [wgpu::RenderPipeline; 4] {
        let pipeline_for = |layout, shader, fragment, blend| {
            Self::pipeline_for(device, format, layout, shader, fragment, blend)
        };
        let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
        [
            pipeline_for(layout, shader, "fs_main", alpha),
            pipeline_for(yuv_layout, shader, "fs_yuv", alpha),
            Self::pipeline_for(
                device,
                wgpu::TextureFormat::Rgba8Unorm,
                blur_layout,
                blur_shader,
                "fs_scene_blur",
                None,
            ),
            pipeline_for(
                blur_layout,
                blur_shader,
                "fs_scene_composite",
                Some(wgpu::BlendState::ALPHA_BLENDING),
            ),
        ]
    }

    /// An empty texture of the right format and usage for
    /// [`BackgroundPipeline::add_texture`], for sRGB-encoded pixels. On an
    /// sRGB target it is an sRGB texture, so sampling returns linear light
    /// like the target expects; on a plain target the encoded values pass
    /// straight through, like every other color in the scene.
    /// `RENDER_ATTACHMENT` is required by WebGPU's
    /// `copyExternalImageToTexture`.
    pub fn create_texture(&self, device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        crate::gpu::texture_2d(
            device,
            "ddi-background-image",
            [width.max(1), height.max(1)],
            if self.srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            1,
            wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            &[],
        )
    }

    fn params_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-background-params"),
            size: self.stride * SLOTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn pipeline_for(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        fragment: &str,
        blend: Option<wgpu::BlendState>,
    ) -> wgpu::RenderPipeline {
        crate::gpu::fullscreen_pipeline(
            device,
            "ddi-background-pipeline",
            layout,
            shader,
            fragment,
            format,
            blend,
        )
    }

    /// One slot of an image's parameters, as its bind group binds it.
    fn params_binding(params: &wgpu::Buffer) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: params,
            offset: 0,
            size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
        })
    }

    /// Registers a movie of `width × height` pixels; returns its id. Its
    /// planes are zeroed, which shows green: register it with the backdrop
    /// only once [`BackgroundPipeline::set_video_frame`] has written a frame.
    pub fn add_video(&mut self, device: &wgpu::Device, width: u32, height: u32) -> usize {
        let params = self.params_buffer(device);
        let (planes, group) = self.video_planes(device, &params, width, height);
        self.images.push(Image {
            group,
            params,
            width: width.max(1) as f32,
            height: height.max(1) as f32,
            video: Some(Video {
                planes,
                bt709: false,
                full_range: false,
                nv12: false,
            }),
        });
        self.images.len() - 1
    }

    /// Three single-channel planes for a `width × height` 4:2:0 frame (odd
    /// sizes round the chroma up) and their bind group.
    fn video_planes(
        &self,
        device: &wgpu::Device,
        params: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> ([wgpu::Texture; 3], wgpu::BindGroup) {
        let (w, h) = (width.max(1), height.max(1));
        let plane = |w: u32, h: u32| {
            crate::gpu::texture_2d(
                device,
                "ddi-background-plane",
                [w, h],
                wgpu::TextureFormat::R8Unorm,
                1,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                &[],
            )
        };
        let planes = [
            plane(w, h),
            plane(w.div_ceil(2), h.div_ceil(2)),
            plane(w.div_ceil(2), h.div_ceil(2)),
        ];
        let views: Vec<_> = planes
            .iter()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-background-yuv-bg"),
            layout: &self.yuv_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: Self::params_binding(params),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&views[2]),
                },
            ],
        });
        (planes, group)
    }

    /// Shows a movie frame in movie `id` (from
    /// [`BackgroundPipeline::add_video`]): `planes` holds Y (`width ×
    /// height`), then U and V (half size, rounded up), rows packed. A frame
    /// of another size replaces the textures. Whether it was written: not
    /// for an empty size, a short buffer or an id that is not a movie.
    #[allow(clippy::too_many_arguments)]
    pub fn set_video_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: usize,
        width: u32,
        height: u32,
        planes: &[u8],
        bt709: bool,
        full_range: bool,
    ) -> bool {
        self.revision = self.revision.wrapping_add(1);
        let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
        let luma = width as usize * height as usize;
        let chroma = cw as usize * ch as usize;
        if width == 0 || height == 0 || planes.len() < luma + 2 * chroma {
            return false;
        }
        let Some(image) = self.images.get(id) else {
            return false;
        };
        if image.video.is_none() {
            return false;
        }
        if image.width != width as f32
            || image.height != height as f32
            || image.video.as_ref().is_some_and(|v| v.nv12)
        {
            let (textures, group) = self.video_planes(device, &image.params, width, height);
            let image = &mut self.images[id];
            image.group = group;
            image.width = width as f32;
            image.height = height as f32;
            if let Some(video) = image.video.as_mut() {
                video.planes = textures;
            }
        }
        let image = &mut self.images[id];
        let Some(video) = image.video.as_mut() else {
            return false;
        };
        video.nv12 = false;
        video.bt709 = bt709;
        video.full_range = full_range;
        let parts = [
            (&video.planes[0], &planes[..luma], width, height),
            (&video.planes[1], &planes[luma..luma + chroma], cw, ch),
            (
                &video.planes[2],
                &planes[luma + chroma..luma + 2 * chroma],
                cw,
                ch,
            ),
        ];
        for (texture, data, w, h) in parts {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        true
    }

    /// Registers a filled texture (from
    /// [`BackgroundPipeline::create_texture`]); returns its id.
    pub fn add_texture(&mut self, device: &wgpu::Device, texture: &wgpu::Texture) -> usize {
        self.revision = self.revision.wrapping_add(1);
        let image = self.make_image(device, texture);
        self.images.push(image);
        self.images.len() - 1
    }

    /// A registered texture was updated in place (for direct RGB movie uploads).
    pub fn texture_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// The color of [`PLAIN`] (sRGB-encoded RGBA; opaque however given).
    pub fn set_plain_color(&mut self, rgba: [u8; 4]) {
        let plain = [rgba[0], rgba[1], rgba[2], 255];
        if self.plain != plain {
            self.plain = plain;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// The image `id` shows ([`PLAIN`] and the black base once made).
    fn image(&self, id: usize) -> Option<&Image> {
        match id {
            PLAIN => self.solids.as_ref().map(|s| &s.plain),
            BLACK => self.solids.as_ref().map(|s| &s.black),
            _ => self.images.get(id),
        }
    }

    /// Makes the one-pixel images, and gives the plain one its color.
    fn solids(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let pixel = |rgba: [u8; 4]| {
            let texture = self.create_texture(device, 1, 1);
            write_pixel(queue, &texture, rgba);
            texture
        };
        if self.solids.is_none() {
            let black = pixel([0, 0, 0, 255]);
            let plain_texture = pixel(self.plain);
            self.solids = Some(Solids {
                black: self.make_image(device, &black),
                plain: self.make_image(device, &plain_texture),
                plain_texture,
                plain_written: self.plain,
            });
        }
        let plain = self.plain;
        if let Some(s) = self.solids.as_mut()
            && s.plain_written != plain
        {
            write_pixel(queue, &s.plain_texture, plain);
            s.plain_written = plain;
        }
    }

    fn make_image(&self, device: &wgpu::Device, texture: &wgpu::Texture) -> Image {
        let params = self.params_buffer(device);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let group = crate::gpu::uniform_texture_group(
            device,
            "ddi-background-bg",
            &self.layout,
            Self::params_binding(&params),
            &view,
            &self.sampler,
        );
        let size = texture.size();
        Image {
            group,
            params,
            width: size.width as f32,
            height: size.height as f32,
            video: None,
        }
    }

    fn make_wide(&self, device: &wgpu::Device, size: (u32, u32)) -> Wide {
        let target = || {
            crate::gpu::texture_2d(
                device,
                "ddi-background-filtered",
                [size.0, size.1],
                wgpu::TextureFormat::Rgba8Unorm,
                1,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
                &[],
            )
            .create_view(&Default::default())
        };
        let (a, b) = (target(), target());
        let params: [wgpu::Buffer; 5] = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ddi-background-filter-params"),
                size: std::mem::size_of::<BlurParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let group = |source: &wgpu::TextureView, params: &wgpu::Buffer| {
            crate::gpu::uniform_texture_group(
                device,
                "ddi-background-filter-group",
                &self.blur_layout,
                params.as_entire_binding(),
                source,
                &self.sampler,
            )
        };
        let groups = [
            group(&a, &params[0]),
            group(&b, &params[1]),
            group(&a, &params[2]),
            group(&a, &params[3]),
            group(&a, &params[4]),
        ];
        Wide {
            size,
            a,
            b,
            params,
            groups,
        }
    }

    /// Uploads the parameters of `backdrop`'s layers for a `width × height`
    /// target, fitted into a frame of shape `frame` (width over height;
    /// `None`: the whole target); returns what to draw.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: f32,
        height: f32,
        brightness: f32,
        backdrop: Backdrop,
        frame: Option<f32>,
        band: Option<([f32; 2], f32)>,
        reveal: ddi_actor::background::Reveal,
    ) -> Prepared {
        use ddi_actor::background::{BASE_ALPHA, BASE_SIDE, CONTENT_SIDE};
        let band =
            band.map(|(edges, keep)| (edges, 1.0 - (1.0 - keep) * reveal.position.clamp(0.0, 1.0)));
        let mut prepared = Prepared {
            target: (width, height),
            alphas: reveal.alphas(),
            ..Default::default()
        };
        if brightness <= 0.0 {
            return prepared;
        }
        let mut layers = backdrop.draws();
        if layers
            .iter()
            .flatten()
            .any(|&(id, _)| id == PLAIN || id == BLACK)
        {
            self.solids(device, queue);
        }
        for layer in &mut layers {
            if !layer.is_some_and(|(id, a)| a > 0.0 && self.image(id).is_some()) {
                *layer = None;
            }
        }
        prepared.layers = layers;
        if layers.iter().all(Option::is_none) {
            return prepared;
        }
        let source_size = self.base_size.unwrap_or_else(|| {
            layers
                .iter()
                .flatten()
                .filter(|&&(id, _)| id != BLACK && id != PLAIN)
                .find_map(|&(id, _)| self.image(id).map(|i| [i.width, i.height]))
                .unwrap_or([width, height])
        });
        self.base_size = Some(source_size);
        let aspect = source_size[0] / source_size[1];
        let size = |side: f32| {
            let side = side.min(source_size[0].max(source_size[1]));
            if aspect >= 1.0 {
                (side as u32, (side / aspect).round().max(1.0) as u32)
            } else {
                ((side * aspect).round().max(1.0) as u32, side as u32)
            }
        };
        if self.wide.as_ref().is_none_or(|w| w.size != size(BASE_SIDE)) {
            self.wide = Some(self.make_wide(device, size(BASE_SIDE)));
            self.filtered[0] = None;
        }
        if reveal.clarity < 1.0
            && self
                .soft
                .as_ref()
                .is_none_or(|w| w.size != size(CONTENT_SIDE))
        {
            self.soft = Some(self.make_wide(device, size(CONTENT_SIDE)));
            self.filtered[1] = None;
        }
        // Transient soft targets go away after the entrance; the base is always present.
        if reveal.clarity >= 1.0 {
            self.soft = None;
            self.filtered[1] = None;
        }
        for (i, present) in [self.wide.is_some(), self.soft.is_some()]
            .into_iter()
            .enumerate()
        {
            if present {
                let key = (self.revision, layers);
                prepared.dirty[i] = self.filtered[i].as_ref() != Some(&key);
                prepared.key = Some(key);
            }
        }
        let rect = reveal.rect(width, height, frame);
        prepared.frame = Some(rect);
        let [_, _, fw, fh] = rect;
        let b = BASE_ALPHA
            + (brightness_factor(brightness) - BASE_ALPHA) * reveal.position.clamp(0.0, 1.0);
        let none = ([0.0, 0.0], 1.0);
        for (k, (id, alpha)) in layers
            .into_iter()
            .enumerate()
            .filter_map(|(k, l)| l.map(|v| (k as u64, v)))
        {
            let image = self.image(id).expect("validated");
            let (bt709, full) = image
                .video
                .as_ref()
                .map_or((false, false), |v| (v.bt709, v.full_range));
            let (kr, kb) = weights(bt709);
            for slot in 0..3u64 {
                let (tw, th, brightness, alpha, linear_out, filter) = if slot == 0 {
                    (
                        fw,
                        fh,
                        b,
                        alpha * prepared.alphas[0],
                        self.srgb,
                        band.unwrap_or(none),
                    )
                } else {
                    (aspect, 1.0, 1.0, alpha, false, none)
                };
                let (uv_scale, uv_offset) = cover_uv(tw, th, image.width, image.height);
                let params = Params {
                    uv_scale,
                    uv_offset,
                    brightness,
                    alpha,
                    kr,
                    kb,
                    full_range: if full { 1.0 } else { 0.0 },
                    linear_out: if linear_out { 1.0 } else { 0.0 },
                    band: filter.0,
                    band_keep: filter.1,
                    source_linear: if self.srgb { 1.0 } else { 0.0 },
                    _pad: [0.0; 2],
                };
                queue.write_buffer(
                    &image.params,
                    (3 * k + slot) * self.stride,
                    bytemuck::bytes_of(&params),
                );
            }
        }
        for (soft, wide) in [(false, self.wide.as_ref()), (true, self.soft.as_ref())] {
            let Some(wide) = wide else { continue };
            for k in 0..5 {
                let content = k >= 3;
                let uv = if content {
                    cover_uv(fw, fh, aspect, 1.0)
                } else {
                    cover_uv(width, height, aspect, 1.0)
                };
                let params = BlurParams {
                    step: match k {
                        0 => [1.0 / wide.size.0 as f32, 0.0],
                        1 => [0.0, 1.0 / wide.size.1 as f32],
                        _ => [0.0; 2],
                    },
                    brightness: if content { b } else { BASE_ALPHA },
                    linear: if self.srgb { 1.0 } else { 0.0 },
                    band: band.map_or([0.0; 2], |v| v.0),
                    band_keep: band.map_or(1.0, |v| v.1),
                    unpremultiply: 0.0,
                    uv_scale: uv.0,
                    uv_offset: uv.1,
                    opacity: if content {
                        prepared.alphas[if soft { 2 } else { 1 }]
                    } else {
                        1.0
                    },
                    base: if content { 0.0 } else { 1.0 },
                    _pad2: [0.0; 2],
                };
                queue.write_buffer(&wide.params[k], 0, bytemuck::bytes_of(&params));
            }
        }
        prepared
    }

    fn draw_layers(&self, pass: &mut wgpu::RenderPass<'_>, prepared: &Prepared, slot: u64) {
        for (k, layer) in prepared.layers.into_iter().enumerate() {
            let Some((id, _)) = layer else { continue };
            // The ambient base keeps image transparency over the shared ground.
            if slot == 1 && id == BLACK {
                continue;
            }
            let Some(image) = self.image(id) else {
                continue;
            };
            #[cfg(feature = "video-planes")]
            if image.video.as_ref().is_some_and(|v| v.nv12) {
                pass.set_pipeline(if slot == 0 {
                    &self.nv12_pipeline
                } else {
                    &self.base_nv12_pipeline
                });
                pass.set_bind_group(
                    0,
                    &image.group,
                    &[((3 * k as u64 + slot) * self.stride) as u32],
                );
                pass.draw(0..6, 0..1);
                continue;
            }
            pass.set_pipeline(match (slot == 0, image.video.is_some()) {
                (true, false) => &self.pipeline,
                (true, true) => &self.yuv_pipeline,
                (false, false) => &self.base_pipeline,
                (false, true) => &self.base_yuv_pipeline,
            });
            pass.set_bind_group(
                0,
                &image.group,
                &[((3 * k as u64 + slot) * self.stride) as u32],
            );
            pass.draw(0..6, 0..1);
        }
    }

    /// Low-resolution ambient base on every frame; a softer content copy only
    /// during the short entrance. Both use the menu's encoded Gaussian kernel.
    pub fn draw_extension(&mut self, encoder: &mut wgpu::CommandEncoder, prepared: &Prepared) {
        if prepared.frame.is_none() {
            return;
        }
        for (slot, wide) in [(1, self.wide.as_ref()), (2, self.soft.as_ref())] {
            if !prepared.dirty[slot as usize - 1] {
                continue;
            }
            let Some(wide) = wide else { continue };
            {
                let mut pass = wide_pass(encoder, &wide.a);
                self.draw_layers(&mut pass, prepared, slot);
            }
            for (group, target) in [(&wide.groups[0], &wide.b), (&wide.groups[1], &wide.a)] {
                let mut pass = wide_pass(encoder, target);
                pass.set_pipeline(&self.blur_pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, 0..1);
            }
            self.filtered[slot as usize - 1] = prepared.key;
            self.filter_jobs[slot as usize - 1] += 1;
        }
    }

    /// Completed Gaussian jobs for the ambient base and transient soft content.
    /// Static scenes reuse both; source frames or crossfade weights invalidate them.
    pub fn filter_jobs(&self) -> [u64; 2] {
        self.filter_jobs
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, prepared: &Prepared) {
        let (Some([x, y, w, h]), Some(wide)) = (prepared.frame, &self.wide) else {
            return;
        };
        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(0, &wide.groups[2], &[]);
        pass.draw(0..3, 0..1);
        pass.set_viewport(x, y, w, h, 0.0, 1.0);
        if prepared.alphas[0] > 0.0 {
            self.draw_layers(pass, prepared, 0);
        }
        if prepared.alphas[1] > 0.0 {
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &wide.groups[3], &[]);
            pass.draw(0..3, 0..1);
        }
        if prepared.alphas[2] > 0.0
            && let Some(soft) = &self.soft
        {
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &soft.groups[4], &[]);
            pass.draw(0..3, 0..1);
        }
        let (w, h) = prepared.target;
        pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
    }
}

/// A pass into one of the filter targets, cleared transparent.
fn wide_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ddi-background-wide"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// Fills a one-pixel texture.
fn write_pixel(queue: &wgpu::Queue, texture: &wgpu::Texture, rgba: [u8; 4]) {
    crate::gpu::write_rgba(queue, texture, [1, 1], &rgba);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_crops_the_overflowing_side_evenly() {
        // A 4:3 image on a 16:9 target: full width, top and bottom cropped.
        let (s, o) = cover_uv(1600.0, 900.0, 640.0, 480.0);
        assert!((s[0] - 1.0).abs() < 1e-6 && (s[1] - 0.75).abs() < 1e-6);
        assert!((o[1] - 0.125).abs() < 1e-6 && o[0] == 0.0);
        // A wide image on a portrait target: full height, sides cropped.
        let (s, o) = cover_uv(400.0, 800.0, 800.0, 400.0);
        assert!((s[0] - 0.25).abs() < 1e-6 && s[1] == 1.0);
        assert!((o[0] - 0.375).abs() < 1e-6);
        // Same aspect: the whole image.
        assert_eq!(
            cover_uv(640.0, 480.0, 320.0, 240.0),
            ([1.0, 1.0], [0.0, 0.0])
        );
    }

    #[test]
    fn the_frame_is_centred_and_as_large_as_fits() {
        // 4:3 on 16:9: full height, bars at the sides.
        assert_eq!(
            frame_rect(1920.0, 1080.0, Some(4.0 / 3.0)),
            Some([240.0, 0.0, 1440.0, 1080.0])
        );
        // 16:9 on a portrait phone: full width, centred vertically.
        assert_eq!(
            frame_rect(900.0, 1600.0, Some(16.0 / 9.0)),
            Some([0.0, 546.875, 900.0, 506.25])
        );
        // The screen's own shape, within a pixel, or none: the whole screen.
        assert_eq!(frame_rect(1920.0, 1080.0, Some(16.0 / 9.0)), None);
        assert_eq!(frame_rect(1366.0, 768.0, Some(16.0 / 9.0)), None);
        assert_eq!(frame_rect(1920.0, 1080.0, None), None);
        assert_eq!(frame_rect(1920.0, 1080.0, Some(f32::NAN)), None);
    }

    #[test]
    fn the_extension_enlarges_what_the_frame_shows() {
        // A 16:9 movie with a 4:3 picture in a 4:3 frame on a 16:9 screen:
        // the frame shows the middle 3/4 of its width ...
        let framed = cover_uv(1440.0, 1080.0, 852.0, 480.0);
        assert!((framed.0[0] - 0.7512).abs() < 1e-3 && framed.0[1] == 1.0);
        // ... and the extension covers the screen with that part, cropped
        // top and bottom to the screen's shape: never the bars at the sides.
        let (s, o) = extension_uv(framed, cover_uv(1920.0, 1080.0, 1440.0, 1080.0));
        assert!((s[0] - framed.0[0]).abs() < 1e-6 && (o[0] - framed.1[0]).abs() < 1e-6);
        assert!((s[1] - 0.75).abs() < 1e-6 && (o[1] - 0.125).abs() < 1e-6);
        // A frame the screen's shape: the extension is the frame's view.
        let same = cover_uv(1920.0, 1080.0, 640.0, 480.0);
        assert_eq!(extension_uv(same, ([1.0, 1.0], [0.0, 0.0])), same);
    }

    #[test]
    fn backdrop_layers_crossfade() {
        assert_eq!(Backdrop::image(2).layers(), [Some((2, 1.0)), None]);
        assert_eq!(Backdrop::default().layers(), [None, None]);
        let fade = |previous, current, mix| Backdrop {
            current,
            previous,
            mix,
            ..Backdrop::default()
        };
        // Image to image, as StepMania draws it: the new one underneath,
        // the old one over it fading out.
        assert_eq!(
            fade(Some(0), Some(1), 0.25).layers(),
            [Some((1, 1.0)), Some((0, 0.75))]
        );
        // From no image, or to no image, over the black base.
        assert_eq!(fade(None, Some(1), 0.25).layers(), [Some((1, 0.25)), None]);
        assert_eq!(fade(Some(0), None, 0.25).layers(), [Some((0, 0.75)), None]);
        // Done, or nothing to fade.
        assert_eq!(fade(Some(0), Some(1), 1.0).layers(), [Some((1, 1.0)), None]);
        assert_eq!(fade(Some(1), Some(1), 0.5).layers(), [Some((1, 1.0)), None]);
    }

    #[test]
    fn the_second_layer_draws_over_the_first() {
        let second = |previous, current, mix| SecondLayer {
            current,
            previous,
            mix,
        };
        // Its current fully shown, the one leaving over it, fading out.
        assert_eq!(
            second(Some(3), Some(4), 0.25).layers(),
            [Some((4, 1.0)), Some((3, 0.75))]
        );
        assert_eq!(second(None, Some(4), 1.0).layers(), [Some((4, 1.0)), None]);
        assert_eq!(second(Some(3), None, 0.5).layers(), [None, Some((3, 0.5))]);
        assert_eq!(SecondLayer::default().layers(), [None, None]);
        // After the first layer's draws, in their places.
        let backdrop = Backdrop {
            current: Some(1),
            previous: Some(0),
            mix: 0.5,
            second: second(Some(1), Some(2), 0.5),
        };
        assert_eq!(
            backdrop.draws(),
            [
                Some((BLACK, 1.0)),
                Some((1, 1.0)),
                Some((0, 0.5)),
                Some((2, 1.0)),
                Some((1, 0.5))
            ]
        );
        // No black base without a first layer to draw.
        let only_second = Backdrop {
            second: second(None, Some(PLAIN), 1.0),
            ..Backdrop::default()
        };
        assert_eq!(
            only_second.draws(),
            [None, None, None, Some((PLAIN, 1.0)), None]
        );
    }

    #[test]
    fn shader_validates() {
        for source in [include_str!("background.wgsl"), include_str!("blur.wgsl")] {
            let module = wgpu::naga::front::wgsl::parse_str(source).expect("WGSL parses");
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .expect("WGSL validates");
        }
        // Params is laid out as the shader reads it.
        assert_eq!(std::mem::size_of::<Params>(), 64);
    }

    /// The shader's conversion transcribed for the CPU gives the reference's
    /// numbers (`ddi_platform::video::yuv_to_rgb`). It checks the formula,
    /// not the WGSL itself: the movie playback test reads frames back from
    /// the screen on both backends (verify skill).
    #[test]
    fn yuv_conversion_matches_the_reference() {
        use ddi_platform::video::{YuvMatrix, yuv_to_rgb};
        let shader = |y: u8, u: u8, v: u8, bt709: bool, full: bool| {
            let (y, u, v) = (f32::from(y), f32::from(u) - 128.0, f32::from(v) - 128.0);
            let (luma, cb, cr) = if full {
                (y / 255.0, u / 255.0, v / 255.0)
            } else {
                ((y - 16.0) / 219.0, u / 224.0, v / 224.0)
            };
            let (kr, kb) = weights(bt709);
            let r = luma + 2.0 * (1.0 - kr) * cr;
            let b = luma + 2.0 * (1.0 - kb) * cb;
            let g = (luma - kr * r - kb * b) / (1.0 - kr - kb);
            [r, g, b].map(|c| c.clamp(0.0, 1.0))
        };
        for (y, u, v) in [
            (16, 128, 128),
            (235, 128, 128),
            (81, 90, 240),
            (63, 102, 240),
            (128, 40, 200),
        ] {
            for (bt709, matrix) in [(false, YuvMatrix::Bt601), (true, YuvMatrix::Bt709)] {
                for full in [false, true] {
                    let a = shader(y, u, v, bt709, full);
                    let b = yuv_to_rgb(y, u, v, matrix, full);
                    assert!(
                        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6),
                        "{a:?} {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn brightness_is_a_multiplier_of_encoded_values() {
        assert_eq!(brightness_factor(0.0), 0.0);
        assert_eq!(brightness_factor(1.0), 1.0);
        assert_eq!(brightness_factor(2.0), 1.0);
        assert_eq!(brightness_factor(0.5), 0.5);
    }
}

#[cfg(feature = "video-bench")]
mod benchmark;

#[cfg(feature = "video-planes")]
mod nv12;
