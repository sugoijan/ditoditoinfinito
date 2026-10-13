//! Owns the pipelines and draws one frame.

use ddi_chart::Layout;
use ddi_engine::frame::Frame;
use ddi_engine::rules::JudgeNames;

use crate::background::BackgroundPipeline;
use crate::color::to_target;
use crate::noteskin::NoteLayer;
use crate::offscreen::{self, Offscreen};
use crate::scene::{self, FieldGeometry, RenderOptions, Segment, Skin};
use crate::sprite::SpritePipeline;
use crate::text::TextPipeline;
use crate::textured::TexturedPipeline;

/// How a noteskin's quads blend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinBlending {
    /// As everything else (in linear light on an sRGB target).
    Scene,
    /// As StepMania's, in encoded values, through this plain view of the
    /// sRGB target ([`Renderer::render`]'s `plain_view`; WebGPU).
    PlainView(wgpu::TextureFormat),
    /// As StepMania's, every frame drawn offscreen with the scene in this
    /// float format (the device must render and blend it) and copied to
    /// the sRGB target (WebGL2, whose surface has no other view).
    Offscreen(wgpu::TextureFormat),
}

pub struct Renderer {
    background: BackgroundPipeline,
    sprites: SpritePipeline,
    textured: TexturedPipeline,
    text: TextPipeline,
    /// [`SkinBlending::Offscreen`]: every frame is drawn offscreen, while
    /// skins are drawn ([`Renderer::set_skins_drawn`]).
    offscreen: Option<Offscreen>,
    /// The target's format, and the float one of [`SkinBlending::Offscreen`].
    format: wgpu::TextureFormat,
    offscreen_format: Option<wgpu::TextureFormat>,
    pub skin: Skin,
    instance_count: usize,
    /// The target format is sRGB (blending and sampling in linear light).
    srgb: bool,
}

impl Renderer {
    /// A renderer drawing to `format`, skins' quads blending as `skins`
    /// says (StepMania's ways only on an sRGB target).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        skins: SkinBlending,
        fonts: &[&'static [u8]],
    ) -> Renderer {
        let srgb = format.is_srgb();
        let skins = match skins {
            SkinBlending::PlainView(p) if srgb && !p.is_srgb() => skins,
            SkinBlending::Offscreen(_) if srgb => skins,
            _ => SkinBlending::Scene,
        };
        // Offscreen, the scene's pipelines draw linear light into the
        // float texture.
        let (scene_format, offscreen) = match skins {
            SkinBlending::Offscreen(f) => (f, Some(Offscreen::new(device, format, f))),
            _ => (format, None),
        };
        let textured = match skins {
            SkinBlending::PlainView(p) => TexturedPipeline::new(device, format, Some(p), false),
            SkinBlending::Offscreen(f) => {
                TexturedPipeline::new(device, f, Some(offscreen::PLAIN), true)
            }
            SkinBlending::Scene => TexturedPipeline::new(device, format, None, false),
        };
        Renderer {
            background: BackgroundPipeline::new(device, scene_format, srgb),
            sprites: SpritePipeline::new(device, scene_format, srgb),
            textured,
            text: TextPipeline::new(device, queue, scene_format, fonts),
            offscreen_format: offscreen.as_ref().map(|_| scene_format),
            offscreen,
            format,
            skin: Skin::default(),
            instance_count: 0,
            srgb,
        }
    }

    /// Whether a skin's quads will be drawn (a session with a skin). With
    /// [`SkinBlending::Offscreen`], frames are drawn offscreen only then:
    /// without a skin the scene's pipelines draw to the target again.
    pub fn set_skins_drawn(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, drawn: bool) {
        let Some(float) = self.offscreen_format else {
            return;
        };
        if drawn == self.offscreen.is_some() {
            return;
        }
        let scene_format = if drawn { float } else { self.format };
        self.sprites.retarget(device, scene_format);
        self.background.retarget(device, scene_format);
        self.text.retarget(device, queue, scene_format);
        self.offscreen = drawn.then(|| Offscreen::new(device, self.format, float));
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
    /// With `mipmaps` (a model's texture), every level, filled by
    /// [`Renderer::generate_noteskin_mipmaps`] once the first is.
    pub fn create_noteskin_texture(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        mipmaps: bool,
    ) -> wgpu::Texture {
        self.textured
            .create_texture(device, width, height, self.srgb, mipmaps)
    }

    /// Fills the mipmaps of a noteskin texture whose first level is filled.
    pub fn generate_noteskin_mipmaps(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
    ) {
        self.textured.generate_mipmaps(device, queue, texture);
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
        let texture = self.create_noteskin_texture(device, art.width, art.height, false);
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
    /// or offscreen in a plain texture, so their colours are given encoded,
    /// as on a plain target).
    pub fn skins_blend_encoded(&self) -> bool {
        self.textured.gamma()
    }

    /// Frames are drawn offscreen (WebGL2 with a skin drawn).
    pub fn offscreen(&self) -> bool {
        self.offscreen.is_some()
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
    /// renderer was made for it, or, without one, the frame offscreen and
    /// copied to `view`.
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
        // the scene's (`skins_blend_encoded`); offscreen, to a plain texture
        // the scene so far is drawn into encoded, then back into the scene
        // decoded, and the frame is drawn to the target at the end.
        // The skin's colours were made encoded for it (`skins_blend_encoded`).
        debug_assert!(
            scene.quads.is_empty()
                || plain_view.is_some()
                || self.offscreen.is_some()
                || !self.textured.gamma(),
            "skins blend encoded: the plain view is needed"
        );
        let plain_view = plain_view.filter(|_| self.textured.gamma());
        let plans = passes(
            &scene.order,
            plain_view.is_some() || self.offscreen.is_some(),
        );
        let clear = self.clear_color();
        let offscreen = self.offscreen.as_mut().map(|o| {
            o.prepare(device, width, height);
            &*o
        });
        // Offscreen, the conversions cover the run's quads only: the plain
        // texture is read there alone.
        let mut run: Option<Rect> = None;
        for (n, pass_plan) in plans.iter().enumerate() {
            if pass_plan.plain {
                run = scene.order[pass_plan.segments.clone()]
                    .iter()
                    .filter_map(|s| match s {
                        Segment::Quads(r) => scene.quads.get(r.start as usize..r.end as usize),
                        Segment::Sprites(_) => None,
                    })
                    .flat_map(|q| q.iter())
                    .filter_map(|q| quad_rect(q, width, height))
                    .reduce(Rect::union);
            }
            let target = match (pass_plan.plain, offscreen, plain_view) {
                (true, Some(o), _) => o.plain(),
                (true, None, Some(plain)) => plain,
                (false, Some(o), _) => o.scene(),
                _ => view,
            };
            let load = match (n, pass_plan.plain, offscreen) {
                (0, _, _) => wgpu::LoadOp::Clear(clear),
                // What the encoding leaves out is never read.
                (_, true, Some(_)) => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                _ => wgpu::LoadOp::Load,
            };
            let mut pass = begin_pass(encoder, target, load);
            if n == 0 {
                self.background.draw(&mut pass, &background);
            }
            // Every pass after the first follows one of the other kind.
            if let (Some(o), Some(r)) = (offscreen, run)
                && n > 0
            {
                pass.set_scissor_rect(r.x, r.y, r.width, r.height);
                if pass_plan.plain {
                    o.scene_to_plain(&mut pass);
                } else {
                    o.plain_to_scene(&mut pass);
                }
                pass.set_scissor_rect(0, 0, width.max(1), height.max(1));
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
        if let Some(o) = offscreen {
            let mut pass = begin_pass(encoder, view, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
            o.scene_to_target(&mut pass);
        }
        self.text.trim();
    }
}

/// A rectangle of the target, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Rect {
    fn union(self, o: Rect) -> Rect {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        let right = (self.x + self.width).max(o.x + o.width);
        let bottom = (self.y + self.height).max(o.y + o.height);
        Rect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
}

/// The pixels a quad or triangle may touch on a `width × height` target
/// (its corners' box, two pixels wider for filtering); `None` off it.
fn quad_rect(q: &crate::textured::TexQuad, width: u32, height: u32) -> Option<Rect> {
    use crate::textured::Shape;
    let corners: Vec<[f32; 2]> = match q.shape {
        Shape::Quad { transform: m, .. } => [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
            .iter()
            .map(|&(u, v)| [m[4] + m[0] * u + m[2] * v, m[5] + m[1] * u + m[3] * v])
            .collect(),
        Shape::Triangle { points, .. } => points.to_vec(),
    };
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for c in &corners {
        if !(c[0].is_finite() && c[1].is_finite()) {
            return None;
        }
        for k in 0..2 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    let x0 = (lo[0] - 2.0).floor().clamp(0.0, width as f32) as u32;
    let y0 = (lo[1] - 2.0).floor().clamp(0.0, height as f32) as u32;
    let x1 = (hi[0] + 2.0).ceil().clamp(0.0, width as f32) as u32;
    let y1 = (hi[1] + 2.0).ceil().clamp(0.0, height as f32) as u32;
    (x1 > x0 && y1 > y0).then_some(Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
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
    fn a_runs_rectangle_covers_its_quads_on_the_target() {
        use crate::textured::{QuadBlend, Shape, TexQuad};
        let quad = |shape| TexQuad {
            shape,
            color: [1.0; 4],
            glow: false,
            texture: 0,
            blend: QuadBlend::Normal,
            wrap: false,
        };
        // 20 × 10 at (100, 50), turned a quarter: 10 wide, 20 high.
        let turned = quad(Shape::Quad {
            transform: [0.0, 20.0, -10.0, 0.0, 100.0, 50.0],
            uv: [0.0; 4],
        });
        let r = quad_rect(&turned, 640, 480).unwrap();
        assert_eq!(
            r,
            Rect {
                x: 93,
                y: 38,
                width: 14,
                height: 24
            }
        );
        // Clipped to the target; off it, nothing.
        let tri = quad(Shape::Triangle {
            points: [[-50.0, -5.0], [10.0, 0.0], [0.0, 10.0]],
            w: [1.0; 3],
            uv: [[0.0; 2]; 3],
        });
        assert_eq!(
            quad_rect(&tri, 640, 480),
            Some(Rect {
                x: 0,
                y: 0,
                width: 12,
                height: 12
            })
        );
        let off = quad(Shape::Quad {
            transform: [10.0, 0.0, 0.0, 10.0, -100.0, 50.0],
            uv: [0.0; 4],
        });
        assert_eq!(quad_rect(&off, 640, 480), None);
        assert_eq!(
            r.union(Rect {
                x: 0,
                y: 0,
                width: 12,
                height: 12
            }),
            Rect {
                x: 0,
                y: 0,
                width: 107,
                height: 62
            }
        );
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
