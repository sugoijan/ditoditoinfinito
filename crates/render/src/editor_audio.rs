//! The chart editor's audio view on the device (`docs/plans/editor.md`,
//! "The audio view"): the picture the editor screen paints (its rows
//! the music's waveform and spectrogram, `ddi_ui::screens::editor::audio`)
//! written into a texture the 2D pass shows under [`SOURCE`], at the
//! physical pixels it was painted for, only when it is a new one.

use crate::Painter;

/// The texture source the 2D pass shows the picture as.
pub const SOURCE: &str = "editor-audio";

/// The picture's texture, while the editor shows one.
#[derive(Default)]
pub struct AudioTexture {
    texture: Option<wgpu::Texture>,
    /// The picture written last.
    serial: Option<u64>,
}

impl AudioTexture {
    /// Writes picture `serial` (`size` pixels of encoded RGBA rows,
    /// straight alpha) unless it is the one written last, the texture
    /// made again for a new size and handed to the painter; `None`: the
    /// painter shows no picture, the texture let go.
    pub fn show(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        painter: &mut Painter,
        picture: Option<(u64, [u32; 2], &[u8])>,
    ) {
        let Some((serial, size, rgba)) = picture else {
            if self.texture.take().is_some() {
                painter.set_external_image(device, SOURCE, None);
            }
            self.serial = None;
            return;
        };
        if self.serial == Some(serial) {
            return;
        }
        let [w, h] = size;
        if w == 0 || h == 0 || rgba.len() != (w * h * 4) as usize {
            return;
        }
        let fits = self
            .texture
            .as_ref()
            .is_some_and(|t| t.width() == w && t.height() == h);
        if !fits {
            let t = crate::gpu::texture_2d(
                device,
                "ddi-editor-audio",
                size,
                wgpu::TextureFormat::Rgba8Unorm,
                1,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                &[],
            );
            crate::gpu::write_rgba(queue, &t, size, rgba);
            painter.set_external_image(device, SOURCE, Some(&t));
            self.texture = Some(t);
        } else if let Some(t) = &self.texture {
            crate::gpu::write_rgba(queue, t, size, rgba);
        }
        self.serial = Some(serial);
    }

    /// The texture (tests read it back).
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.texture.as_ref()
    }
}
