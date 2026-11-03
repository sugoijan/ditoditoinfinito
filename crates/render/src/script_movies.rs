//! A song's scripts' movies drawn into textures their quads sample
//! (`docs/plans/phase-8-gaps.md`, step 3). The shells' movie decks put a
//! movie's frames on [`ScriptMovies::renderer_mut`] as they put a play's
//! background movies on its renderer (YUV planes, NV12 planes, the
//! browser's own frames); [`ScriptMovies::draw`] records the latest frame
//! into the movie's texture of the play's renderer
//! ([`Renderer::add_movie_target`]), in encoded values (as song select's
//! preview movie is drawn, [`crate::movie_picture`]), which a sprite of
//! the movie samples as it samples an image.
//!
//! The converter has a renderer of its own: the backdrop's drawing keeps
//! state for the frame it prepares (its sizes, its blurred copies, one
//! buffer of parameters), which a play's backdrop must not share. Each
//! drawing is submitted on its own, so two movies drawn in one frame do
//! not share that buffer either.

use crate::background::Backdrop;
use crate::renderer::{Renderer, SkinBlending};

/// The format movies are drawn in: encoded values, through the plain view
/// of their texture.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A movie's texture in the play's renderer.
struct Target {
    /// Its id among the play's renderer's textures.
    id: u32,
    size: [u32; 2],
    /// The plain view drawn into.
    view: wgpu::TextureView,
}

/// The converter and each movie's texture, by the movie's index among the
/// song's ([`ddi_actor::movie_texture`]).
pub struct ScriptMovies {
    converter: Renderer,
    targets: Vec<Option<Target>>,
}

impl ScriptMovies {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> ScriptMovies {
        ScriptMovies {
            converter: Renderer::new(device, queue, FORMAT, SkinBlending::Scene, &[]),
            targets: Vec::new(),
        }
    }

    /// The renderer the movies' frames go on (`Renderer::add_video`,
    /// `Renderer::set_video_frame`, a browser frame's texture).
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.converter
    }

    /// Movie `index`'s texture in `play` (the play's renderer), made at
    /// `size` pixels the first time it is asked for (and again after
    /// [`ScriptMovies::forget_targets`]).
    pub fn target(
        &mut self,
        play: &mut Renderer,
        device: &wgpu::Device,
        index: usize,
        size: [u32; 2],
    ) -> u32 {
        if self.targets.len() <= index {
            self.targets.resize_with(index + 1, || None);
        }
        if let Some(t) = &self.targets[index] {
            return t.id;
        }
        let size = size.map(|v| v.max(1));
        let (id, texture) = play.add_movie_target(device, size[0], size[1]);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(FORMAT),
            ..Default::default()
        });
        self.targets[index] = Some(Target { id, size, view });
        id
    }

    /// Each movie's texture id in the play's renderer, by the movie's
    /// index (`None`: not made yet: it is once a frame of it shows).
    pub fn ids(&self) -> Vec<Option<u32>> {
        self.targets
            .iter()
            .map(|t| t.as_ref().map(|t| t.id))
            .collect()
    }

    /// The play's renderer's textures were cleared (a skin replaced
    /// them), these with them: they are made again as asked for.
    pub fn forget_targets(&mut self) {
        self.targets.clear();
    }

    /// Records movie `index`'s frame in the converter's movie `video` into
    /// its texture, covering it, and submits it. Nothing before the
    /// texture is made.
    pub fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, index: usize, video: usize) {
        let Some(Some(target)) = self.targets.get(index) else {
            return;
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ddi-script-movie"),
        });
        self.converter.render_backdrop(
            device,
            queue,
            &mut encoder,
            &target.view,
            target.size[0],
            target.size[1],
            Backdrop::image(video),
        );
        queue.submit(Some(encoder.finish()));
    }
}
