//! A movie drawn alone into a texture the 2D pass shows as an image: song
//! select's preview movie (`#PREVIEWVID`, `docs/plans/lane-covers.md`,
//! "Preview video"). The shells' movie decks put its frames on
//! [`MoviePicture::renderer_mut`] as they put a play's on its renderer
//! (YUV planes, NV12 planes, the browser's own frames), and
//! [`MoviePicture::draw`] records the frame into a texture of the movie's
//! shape, in encoded values (as Options' field preview's,
//! [`crate::field_preview`]), which the painter shows under [`SOURCE`]
//! ([`crate::Painter::set_external_image`]), cover-fitted into the
//! banner's place.

use crate::background::Backdrop;
use crate::renderer::{Renderer, SkinBlending};

/// The texture source the 2D pass draws the movie under
/// (`ddi_ui::screens::select::PREVIEW_MOVIE`).
pub const SOURCE: &str = "preview-movie";

/// The picture's format: encoded values, as the 2D pass's images.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The picture's longer side at most, pixels: the banner's place is
/// smaller than this on any screen.
pub const MAX_SIDE: u32 = 1280;

struct Target {
    size: [u32; 2],
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// A renderer for one movie at a time and the texture it is drawn into.
pub struct MoviePicture {
    renderer: Renderer,
    target: Option<Target>,
    /// Changes each time the texture is made again (the painter's image
    /// is set again then).
    generation: u64,
}

impl MoviePicture {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> MoviePicture {
        MoviePicture {
            renderer: Renderer::new(device, queue, FORMAT, SkinBlending::Scene, &[]),
            target: None,
            generation: 0,
        }
    }

    /// The renderer the movie's frames go on (`Renderer::add_video`,
    /// `Renderer::set_video_frame`, a browser frame's texture).
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    /// Another movie comes: the last one's textures go (the picture keeps
    /// what it shows until it is drawn again).
    pub fn clear(&mut self) {
        self.renderer.clear_backgrounds();
    }

    /// The texture drawn into, once a frame was.
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|t| &t.texture)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Lets the texture go (nothing shows it any more).
    pub fn release(&mut self) {
        self.target = None;
    }

    /// Records movie `id` (from the renderer) as its latest frame shows it
    /// into the picture, at its shape ([`MAX_SIDE`] at most).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        id: usize,
    ) {
        let Some([width, height]) = self.renderer.background_size(id) else {
            return;
        };
        let size = picture_size(width, height);
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let texture = crate::gpu::texture_2d(
                device,
                "ddi-movie-picture",
                size,
                FORMAT,
                1,
                // Read back by tests.
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                &[],
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some(Target {
                size,
                texture,
                view,
            });
            self.generation += 1;
        }
        let Some(target) = &self.target else {
            return;
        };
        self.renderer.render_backdrop(
            device,
            queue,
            encoder,
            &target.view,
            size[0],
            size[1],
            Backdrop::image(id),
        );
    }
}

/// The picture's size for a `width × height` movie: its own, scaled down
/// to [`MAX_SIDE`].
pub fn picture_size(width: u32, height: u32) -> [u32; 2] {
    let (w, h) = (width.max(1), height.max(1));
    let long = w.max(h);
    if long <= MAX_SIDE {
        return [w, h];
    }
    let scale = MAX_SIDE as f64 / long as f64;
    [
        ((w as f64 * scale).round() as u32).max(1),
        ((h as f64 * scale).round() as u32).max(1),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_keeps_the_movies_shape_within_its_largest_side() {
        assert_eq!(picture_size(640, 360), [640, 360]);
        assert_eq!(picture_size(1920, 1080), [1280, 720]);
        assert_eq!(picture_size(1080, 1920), [720, 1280]);
        assert_eq!(picture_size(0, 0), [1, 1]);
    }
}
