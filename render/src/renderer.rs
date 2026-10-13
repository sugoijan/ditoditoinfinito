//! Owns the pipelines and draws one frame.

use ddi_chart::Layout;
use ddi_engine::frame::Frame;
use ddi_engine::rules::JudgeNames;

use crate::background::BackgroundPipeline;
use crate::color::to_target;
use crate::noteskin::NoteLayer;
use crate::scene::{self, FieldGeometry, RenderOptions, Segment, Skin};
use crate::sprite::SpritePipeline;
use crate::text::TextPipeline;
use crate::textured::TexturedPipeline;

pub struct Renderer {
    background: BackgroundPipeline,
    sprites: SpritePipeline,
    textured: TexturedPipeline,
    text: TextPipeline,
    pub skin: Skin,
    instance_count: usize,
    /// The target format is sRGB (blending and sampling in linear light).
    srgb: bool,
}

impl Renderer {
    /// A renderer drawing to `format`; with `plain`, the target's plain
    /// (non-sRGB) format when `format` is an sRGB view of it, skins' quads
    /// are drawn through that view, blending as StepMania blends
    /// ([`Renderer::render`]'s `plain_view`).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        plain: Option<wgpu::TextureFormat>,
        fonts: &[&'static [u8]],
    ) -> Renderer {
        let plain = plain.filter(|p| format.is_srgb() && !p.is_srgb());
        Renderer {
            background: BackgroundPipeline::new(device, format),
            sprites: SpritePipeline::new(device, format),
            textured: TexturedPipeline::new(device, format, plain),
            text: TextPipeline::new(device, queue, format, fonts),
            skin: Skin::default(),
            instance_count: 0,
            srgb: format.is_srgb(),
        }
    }

    /// A texture for [`Renderer::set_background`], to be filled by the shell
    /// with sRGB-encoded pixels.
    pub fn create_background_texture(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> wgpu::Texture {
        self.background.create_texture(device, width, height)
    }

    /// Registers a filled background texture; returns the id a
    /// [`Backdrop`](crate::Backdrop) shows it by.
    pub fn add_background(&mut self, device: &wgpu::Device, texture: &wgpu::Texture) -> usize {
        self.background.add_texture(device, texture)
    }

    /// Registers a movie for [`Renderer::set_video_frame`]; returns the id a
    /// [`Backdrop`](crate::Backdrop) shows it by.
    pub fn add_video(&mut self, device: &wgpu::Device, width: u32, height: u32) -> usize {
        self.background.add_video(device, width, height)
    }

    /// Shows a decoded movie frame (8-bit YUV 4:2:0 planes) in movie `id`;
    /// whether it was written.
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
        self.background
            .set_video_frame(device, queue, id, width, height, planes, bt709, full_range)
    }

    /// A texture for a noteskin image ([`Renderer::add_noteskin_texture`]),
    /// filled by the shell with sRGB-encoded pixels.
    pub fn create_noteskin_texture(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> wgpu::Texture {
        self.textured
            .create_texture(device, width, height, self.srgb)
    }

    /// Registers a filled noteskin texture; returns the id its quads use.
    pub fn add_noteskin_texture(&mut self, device: &wgpu::Device, texture: &wgpu::Texture) -> u32 {
        self.textured.add_texture(device, texture)
    }

    /// Registers the built-in art standing in for one of StepMania's own
    /// skins' images (`ddi_noteskin::build::TextureKey::BuiltIn`).
    pub fn add_builtin_art(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        name: &str,
    ) -> u32 {
        let art = crate::art::builtin(name);
        let texture = self.create_noteskin_texture(device, art.width, art.height);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &art.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(art.width * 4),
                rows_per_image: Some(art.height),
            },
            wgpu::Extent3d {
                width: art.width,
                height: art.height,
                depth_or_array_layers: 1,
            },
        );
        self.textured.add_texture(device, &texture)
    }

    /// Forgets the noteskin textures (another skin, or none).
    pub fn clear_noteskin_textures(&mut self) {
        self.textured.clear_textures();
    }

    /// The target format is sRGB.
    pub fn srgb(&self) -> bool {
        self.srgb
    }

    /// Skins' quads blend as StepMania blends (in the target's plain view,
    /// so their colours are given encoded, as on a plain target).
    pub fn skins_blend_encoded(&self) -> bool {
        self.textured.gamma()
    }

    /// The skin's background colour as the target expects it.
    pub fn clear_color(&self) -> wgpu::Color {
        let c = self.skin.background;
        let [r, g, b, a] = to_target([c.r as f32, c.g as f32, c.b as f32, c.a as f32], self.srgb);
        wgpu::Color {
            r: f64::from(r),
            g: f64::from(g),
            b: f64::from(b),
            a: f64::from(a),
        }
    }

    /// Encode a full frame into `view` (`width × height` pixels); skins'
    /// quads into `plain_view`, the same texture's plain view, when the
    /// renderer was made for it.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        plain_view: Option<&wgpu::TextureView>,
        width: u32,
        height: u32,
        frame: &Frame,
        layout: &Layout,
        names: &JudgeNames,
        opts: RenderOptions,
        layer: Option<&mut dyn NoteLayer>,
    ) {
        let geo = FieldGeometry::new(
            width as f32,
            height as f32,
            scene::field_span(layout),
            opts.reverse,
        );
        let mut scene = scene::build(
            frame, layout, names, &self.skin, &geo, opts, self.srgb, layer,
        );
        for i in &mut scene.instances {
            i.color = to_target(i.color, self.srgb);
            if i.shape == crate::sprite::Shape::ArrowGradient as u32
                || i.shape == crate::sprite::Shape::ArrowRamp as u32
            {
                let [r, g, b, phase] = i.params;
                let [r, g, b, _] = to_target([r, g, b, 1.0], self.srgb);
                i.params = [r, g, b, phase];
            }
        }
        for t in &mut scene.text {
            // Text keeps the look it had on a plain target, where glyphon's
            // accurate colour mode showed it in linear light.
            t.color = to_target(t.color, self.srgb);
        }
        let background = self.background.prepare(
            device,
            queue,
            width as f32,
            height as f32,
            opts.background,
            opts.backdrop,
            opts.frame_aspect,
        );
        self.background.draw_extension(encoder, &background);
        self.sprites
            .set_viewport(queue, width as f32, height as f32, frame.song_time as f32);
        self.sprites.upload(device, queue, &scene.instances);
        self.instance_count = scene.instances.len();
        self.textured
            .prepare(device, queue, width as f32, height as f32, &scene.quads);
        let _ = self.text.prepare(device, queue, width, height, &scene.text);
        // Skins' quads go to the plain view, in passes of their own between
        // the scene's (`skins_blend_encoded`).
        // The skin's colours were made encoded for it (`skins_blend_encoded`).
        debug_assert!(
            plain_view.is_some() || !self.textured.gamma(),
            "skins blend encoded: the plain view is needed"
        );
        let plain_view = plain_view.filter(|_| self.textured.gamma());
        for (n, pass_plan) in passes(&scene.order, plain_view.is_some())
            .iter()
            .enumerate()
        {
            let target = match (pass_plan.plain, plain_view) {
                (true, Some(plain)) => plain,
                _ => view,
            };
            let load = if n == 0 {
                wgpu::LoadOp::Clear(self.clear_color())
            } else {
                wgpu::LoadOp::Load
            };
            let mut pass = begin_pass(encoder, target, load);
            if n == 0 {
                self.background.draw(&mut pass, &background);
            }
            for segment in &scene.order[pass_plan.segments.clone()] {
                match segment {
                    Segment::Sprites(r) => self.sprites.draw_range(&mut pass, r.clone()),
                    Segment::Quads(r) => self.textured.draw(&mut pass, r.clone(), pass_plan.plain),
                }
            }
            if pass_plan.last {
                self.text.draw(&mut pass);
            }
        }
        self.text.trim();
    }
}

/// One render pass of a frame.
#[derive(Clone, Debug, PartialEq)]
struct PassPlan {
    /// Through the plain view (a skin's quads, blending as StepMania's).
    plain: bool,
    /// The scene's segments it draws.
    segments: std::ops::Range<usize>,
    /// The frame's last pass (the text goes on top).
    last: bool,
}

/// The passes a frame's `order` is drawn in: one, or with `plain` the
/// skins' quads in passes of their own on the plain view between the
/// scene's. The first pass clears and draws the background; the last
/// draws the text, on the scene's view.
fn passes(order: &[Segment], plain: bool) -> Vec<PassPlan> {
    let mut out = vec![PassPlan {
        plain: false,
        segments: 0..0,
        last: false,
    }];
    for (i, segment) in order.iter().enumerate() {
        let wants_plain = plain && matches!(segment, Segment::Quads(_));
        let current = out.last_mut().expect("one pass at least");
        if current.plain == wants_plain {
            current.segments.end = i + 1;
        } else {
            out.push(PassPlan {
                plain: wants_plain,
                segments: i..i + 1,
                last: false,
            });
        }
    }
    if out.last().is_some_and(|p| p.plain) {
        out.push(PassPlan {
            plain: false,
            segments: order.len()..order.len(),
            last: false,
        });
    }
    if let Some(p) = out.last_mut() {
        p.last = true;
    }
    out
}

/// A pass drawing to `target`.
fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ddi-scene"),
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(plain: bool, segments: std::ops::Range<usize>, last: bool) -> PassPlan {
        PassPlan {
            plain,
            segments,
            last,
        }
    }

    #[test]
    fn skins_quads_get_passes_of_their_own() {
        use Segment::{Quads, Sprites};
        let order = [
            Sprites(0..2),
            Quads(0..3),
            Quads(3..4),
            Sprites(2..5),
            Quads(4..6),
        ];
        // One pass without a plain view.
        assert_eq!(passes(&order, false), [plan(false, 0..5, true)]);
        // With one: the quads in plain passes, the text in a last scene pass.
        assert_eq!(
            passes(&order, true),
            [
                plan(false, 0..1, false),
                plan(true, 1..3, false),
                plan(false, 3..4, false),
                plan(true, 4..5, false),
                plan(false, 5..5, true),
            ]
        );
        // A frame starting with quads still clears first, on the scene's view.
        assert_eq!(
            passes(&[Quads(0..1)], true),
            [
                plan(false, 0..0, false),
                plan(true, 0..1, false),
                plan(false, 1..1, true),
            ]
        );
        assert_eq!(passes(&[], true), [plan(false, 0..0, true)]);
    }
}
