//! Owns the pipelines and draws one frame.

use ddi_chart::Layout;
use ddi_engine::frame::Frame;
use ddi_engine::rules::JudgeNames;

use crate::scene::{self, FieldGeometry, RenderOptions, Skin};
use crate::sprite::SpritePipeline;
use crate::text::TextPipeline;

pub struct Renderer {
    sprites: SpritePipeline,
    text: TextPipeline,
    pub skin: Skin,
    instance_count: usize,
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        fonts: &[&'static [u8]],
    ) -> Renderer {
        Renderer {
            sprites: SpritePipeline::new(device, format),
            text: TextPipeline::new(device, queue, format, fonts),
            skin: Skin::default(),
            instance_count: 0,
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
        let scene = scene::build(frame, layout, names, &self.skin, &geo, opts);
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
                        load: wgpu::LoadOp::Clear(self.skin.background),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.sprites.draw(&mut pass, self.instance_count);
            self.text.draw(&mut pass);
        }
        self.text.trim();
    }
}
