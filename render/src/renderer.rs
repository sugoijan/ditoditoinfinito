//! Owns the pipelines and draws one frame.

use ddi_chart::Layout;
use ddi_engine::frame::Frame;
use ddi_engine::rules::JudgeNames;

use crate::background::BackgroundPipeline;
use crate::color::to_target;
use crate::scene::{self, FieldGeometry, RenderOptions, Skin};
use crate::sprite::SpritePipeline;
use crate::text::TextPipeline;

pub struct Renderer {
    background: BackgroundPipeline,
    sprites: SpritePipeline,
    text: TextPipeline,
    pub skin: Skin,
    instance_count: usize,
    /// The target format is sRGB (blending and sampling in linear light).
    srgb: bool,
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        fonts: &[&'static [u8]],
    ) -> Renderer {
        Renderer {
            background: BackgroundPipeline::new(device, format),
            sprites: SpritePipeline::new(device, format),
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

    /// Encode a full frame into `view` (`width × height` pixels).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        frame: &Frame,
        layout: &Layout,
        names: &JudgeNames,
        opts: RenderOptions,
    ) {
        let geo = FieldGeometry::new(
            width as f32,
            height as f32,
            layout.lanes.len(),
            opts.reverse,
        );
        let mut scene = scene::build(frame, layout, names, &self.skin, &geo, opts, self.srgb);
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
        let background_layers = self.background.prepare(
            queue,
            width as f32,
            height as f32,
            opts.background,
            opts.backdrop,
        );
        self.sprites
            .set_viewport(queue, width as f32, height as f32, frame.song_time as f32);
        self.sprites.upload(device, queue, &scene.instances);
        self.instance_count = scene.instances.len();
        let _ = self.text.prepare(device, queue, width, height, &scene.text);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ddi-scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.background.draw(&mut pass, background_layers);
            self.sprites.draw(&mut pass, self.instance_count);
            self.text.draw(&mut pass);
        }
        self.text.trim();
    }
}
