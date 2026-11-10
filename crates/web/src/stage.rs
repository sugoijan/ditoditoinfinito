//! A Dancing☆Onigiri work's back and mask layers in play
//! (`docs/plans/overlays.md`): the shared view (`ddi_shell::stage`) over
//! the images and text a song's layers show, decoded by the browser once
//! per song (a retry keeps them) and copied to their textures by it.

use ddi_shell::stage::StageContent;

/// A content decoded (an image, or text drawn), closed once dropped:
/// replaced, or with the view.
pub(crate) struct Bitmap(pub(crate) web_sys::ImageBitmap);

impl StageContent for Bitmap {
    fn size(&self) -> [u32; 2] {
        Image(&self.0).size()
    }

    fn write(&self, queue: &wgpu::Queue, texture: &wgpu::Texture, size: [u32; 2]) {
        Image(&self.0).write(queue, texture, size);
    }
}

/// An image another owner closes, as a content to copy.
pub(crate) struct Image<'a>(pub(crate) &'a web_sys::ImageBitmap);

impl StageContent for Image<'_> {
    fn size(&self) -> [u32; 2] {
        [self.0.width(), self.0.height()]
    }

    fn write(&self, queue: &wgpu::Queue, texture: &wgpu::Texture, [w, h]: [u32; 2]) {
        crate::web::gfx::copy_bitmap(queue, self.0, texture, w, h);
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        self.0.close();
    }
}
