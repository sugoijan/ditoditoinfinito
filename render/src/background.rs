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

use bytemuck::{Pod, Zeroable};

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
    _pad: [f32; 2],
}

/// What to show behind the field in one frame. While `mix < 1` a crossfade
/// is running from `previous` to `current` (`mix` = how far, 1 = only
/// `current`). Ids are from [`BackgroundPipeline::add_texture`]; `None` is
/// no image, the plain field colour.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Backdrop {
    pub current: Option<usize>,
    pub previous: Option<usize>,
    pub mix: f32,
}

impl Backdrop {
    /// One image, fully shown.
    pub fn image(id: usize) -> Backdrop {
        Backdrop {
            current: Some(id),
            previous: None,
            mix: 1.0,
        }
    }

    /// The draws, back to front: `(id, opacity)`.
    pub fn layers(&self) -> [Option<(usize, f32)>; 2] {
        let mix = self.mix.clamp(0.0, 1.0);
        if mix >= 1.0 || self.previous == self.current {
            return [self.current.map(|c| (c, 1.0)), None];
        }
        match (self.previous, self.current) {
            (Some(p), Some(c)) => [Some((p, 1.0)), Some((c, mix))],
            (None, Some(c)) => [Some((c, mix)), None],
            (Some(p), None) => [Some((p, 1.0 - mix)), None],
            (None, None) => [None, None],
        }
    }
}

/// One registered image or movie.
struct Image {
    group: wgpu::BindGroup,
    params: wgpu::Buffer,
    width: f32,
    height: f32,
    /// A movie: its planes and the current frame's colour tags.
    video: Option<Video>,
}

struct Video {
    /// Y, U, V.
    planes: [wgpu::Texture; 3],
    /// BT.709 (else BT.601).
    bt709: bool,
    full_range: bool,
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
    if target_w <= 0.0 || target_h <= 0.0 || image_w <= 0.0 || image_h <= 0.0 {
        return ([1.0, 1.0], [0.0, 0.0]);
    }
    let target = target_w / target_h;
    let image = image_w / image_h;
    if image > target {
        let s = target / image;
        ([s, 1.0], [(1.0 - s) / 2.0, 0.0])
    } else {
        let s = image / target;
        ([1.0, s], [0.0, (1.0 - s) / 2.0])
    }
}

/// Multiplier for a perceived brightness in `0..=1`. On an sRGB target the
/// shader works in linear light, where 0.4 would look much brighter than
/// 40 %; on a plain target the image's values pass through encoded.
pub fn brightness_factor(perceived: f32, srgb_target: bool) -> f32 {
    let p = perceived.clamp(0.0, 1.0);
    if srgb_target { p.powf(2.2) } else { p }
}

pub struct BackgroundPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    yuv_pipeline: wgpu::RenderPipeline,
    yuv_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    images: Vec<Image>,
    /// The target format is sRGB.
    srgb: bool,
}

impl BackgroundPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> BackgroundPipeline {
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
        let common = [
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
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
        let pipeline_for = |layout: &wgpu::BindGroupLayout, fragment: &str| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ddi-background-layout"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ddi-background-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
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
        };
        let pipeline = pipeline_for(&layout, "fs_main");
        let yuv_pipeline = pipeline_for(&yuv_layout, "fs_yuv");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ddi-background-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        BackgroundPipeline {
            pipeline,
            layout,
            yuv_pipeline,
            yuv_layout,
            sampler,
            images: Vec::new(),
            srgb: format.is_srgb(),
        }
    }

    /// An empty texture of the right format and usage for
    /// [`BackgroundPipeline::set_texture`], for sRGB-encoded pixels. On an
    /// sRGB target it is an sRGB texture, so sampling returns linear light
    /// like the target expects; on a plain target the encoded values pass
    /// straight through, like every other colour in the scene.
    /// `RENDER_ATTACHMENT` is required by WebGPU's
    /// `copyExternalImageToTexture`.
    pub fn create_texture(&self, device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ddi-background-image"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if self.srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    fn params_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddi-background-params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Registers a movie of `width × height` pixels; returns its id. Its
    /// planes are zeroed, which shows green: register it with the backdrop
    /// only once [`BackgroundPipeline::set_video_frame`] has written a frame.
    pub fn add_video(&mut self, device: &wgpu::Device, width: u32, height: u32) -> usize {
        let params = Self::params_buffer(device);
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
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ddi-background-plane"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
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
                    resource: params.as_entire_binding(),
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
        if image.width != width as f32 || image.height != height as f32 {
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
        let params = Self::params_buffer(device);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ddi-background-bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let size = texture.size();
        self.images.push(Image {
            group,
            params,
            width: size.width as f32,
            height: size.height as f32,
            video: None,
        });
        self.images.len() - 1
    }

    /// Uploads the parameters of `backdrop`'s layers for a `width × height`
    /// target; returns the layers to draw.
    pub fn prepare(
        &self,
        queue: &wgpu::Queue,
        width: f32,
        height: f32,
        brightness: f32,
        backdrop: Backdrop,
    ) -> [Option<(usize, f32)>; 2] {
        if brightness <= 0.0 {
            return [None, None];
        }
        let mut layers = backdrop.layers();
        for layer in &mut layers {
            let Some((id, alpha)) = *layer else {
                continue;
            };
            let Some(image) = self.images.get(id).filter(|_| alpha > 0.0) else {
                *layer = None;
                continue;
            };
            let (uv_scale, uv_offset) = cover_uv(width, height, image.width, image.height);
            let (bt709, full_range) = image
                .video
                .as_ref()
                .map_or((false, false), |v| (v.bt709, v.full_range));
            let (kr, kb) = weights(bt709);
            let p = Params {
                uv_scale,
                uv_offset,
                brightness: brightness_factor(brightness, self.srgb),
                alpha,
                kr,
                kb,
                full_range: if full_range { 1.0 } else { 0.0 },
                linear_out: if self.srgb { 1.0 } else { 0.0 },
                _pad: [0.0; 2],
            };
            queue.write_buffer(&image.params, 0, bytemuck::bytes_of(&p));
        }
        layers
    }

    /// Draws the layers [`BackgroundPipeline::prepare`] returned.
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, layers: [Option<(usize, f32)>; 2]) {
        for (id, _) in layers.into_iter().flatten() {
            if let Some(image) = self.images.get(id) {
                pass.set_pipeline(if image.video.is_some() {
                    &self.yuv_pipeline
                } else {
                    &self.pipeline
                });
                pass.set_bind_group(0, &image.group, &[]);
                pass.draw(0..6, 0..1);
            }
        }
    }
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
    fn backdrop_layers_crossfade() {
        assert_eq!(Backdrop::image(2).layers(), [Some((2, 1.0)), None]);
        assert_eq!(Backdrop::default().layers(), [None, None]);
        let fade = |previous, current, mix| Backdrop {
            current,
            previous,
            mix,
        };
        // Image to image: the old one underneath, the new one fading in.
        assert_eq!(
            fade(Some(0), Some(1), 0.25).layers(),
            [Some((0, 1.0)), Some((1, 0.25))]
        );
        // From no image, or to no image, over the field colour.
        assert_eq!(fade(None, Some(1), 0.25).layers(), [Some((1, 0.25)), None]);
        assert_eq!(fade(Some(0), None, 0.25).layers(), [Some((0, 0.75)), None]);
        // Done, or nothing to fade.
        assert_eq!(fade(Some(0), Some(1), 1.0).layers(), [Some((1, 1.0)), None]);
        assert_eq!(fade(Some(1), Some(1), 0.5).layers(), [Some((1, 1.0)), None]);
    }

    #[test]
    fn shader_validates() {
        let source = include_str!("background.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("WGSL validates");
        // Params is laid out as the shader reads it.
        assert_eq!(std::mem::size_of::<Params>(), 48);
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
    fn brightness_is_perceptual_on_either_target() {
        for srgb in [false, true] {
            assert_eq!(brightness_factor(0.0, srgb), 0.0);
            assert_eq!(brightness_factor(1.0, srgb), 1.0);
            assert_eq!(brightness_factor(2.0, srgb), 1.0);
        }
        assert!((brightness_factor(0.5, true) - 0.5f32.powf(2.2)).abs() < 1e-6);
        assert_eq!(brightness_factor(0.5, false), 0.5);
    }
}
