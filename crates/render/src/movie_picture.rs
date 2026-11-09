//! A movie drawn alone into a texture the 2D pass shows as an image: song
//! select's preview movie (`#PREVIEWVID`, `docs/plans/lane-covers.md`,
//! "Preview video"). The shells' movie decks put its frames on
//! [`MoviePicture::renderer_mut`] as they put a play's on its background
//! pipeline (YUV planes, NV12 planes, the browser's own frames), and
//! [`MoviePicture::draw`] records the frame into a texture of the movie's
//! shape, in encoded values (as Options' field preview's,
//! [`crate::field_preview`]), which the painter shows under [`SOURCE`]
//! ([`crate::Painter::set_external_image`]), cover-fitted into the
//! banner's place.

use crate::background::{Backdrop, BackgroundPipeline};

/// The texture source the 2D pass draws the movie under
/// (`ddi_ui::screens::select::PREVIEW_MOVIE`).
pub const SOURCE: &str = "preview-movie";

/// The picture's format: encoded values, as the 2D pass's images.
pub const FORMAT: wgpu::TextureFormat = crate::gpu::PICTURE_FORMAT;

/// The picture's longer side at most, pixels: the banner's place is
/// smaller than this on any screen.
pub const MAX_SIDE: u32 = 1280;

/// A background pipeline for one movie at a time and the texture it is
/// drawn into.
pub struct MoviePicture {
    background: BackgroundPipeline,
    target: crate::gpu::PictureTarget,
}

impl MoviePicture {
    pub fn new(device: &wgpu::Device) -> MoviePicture {
        MoviePicture {
            background: BackgroundPipeline::new(device, FORMAT, FORMAT.is_srgb()),
            target: crate::gpu::PictureTarget::default(),
        }
    }

    /// The pipeline the movie's frames go on
    /// ([`BackgroundPipeline::add_video`],
    /// [`BackgroundPipeline::set_video_frame`], a browser frame's texture).
    pub fn renderer_mut(&mut self) -> &mut BackgroundPipeline {
        &mut self.background
    }

    /// Another movie comes: the last one's textures go (the picture keeps
    /// what it shows until it is drawn again).
    pub fn clear(&mut self) {
        self.background.clear_images();
    }

    /// The texture drawn into, once a frame was.
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.target.texture()
    }

    pub fn generation(&self) -> u64 {
        self.target.generation()
    }

    /// Lets the texture go (nothing shows it any more).
    pub fn release(&mut self) {
        self.target.release();
    }

    /// Records movie `id` (from the pipeline) as its latest frame shows it
    /// into the picture, at its shape ([`MAX_SIDE`] at most).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        id: usize,
    ) {
        let Some([width, height]) = self.background.image_size(id) else {
            return;
        };
        let size = picture_size(width, height);
        self.target
            .ensure(device, "ddi-movie-picture", size, FORMAT);
        let Some(view) = self.target.view() else {
            return;
        };
        self.background.render_backdrop(
            device,
            queue,
            encoder,
            view,
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
