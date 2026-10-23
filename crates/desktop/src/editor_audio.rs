//! The chart editor's audio view on desktop (`docs/plans/editor.md`,
//! "The audio view"): the picture the editor screen painted, handed
//! over by its host ([`show`]), written into the texture the painter
//! shows as `editor-audio` before the UI's frame, only when it is a new
//! one (`ddi_render::editor_audio`).

use std::cell::RefCell;

use ddi_render::Painter;
use ddi_render::editor_audio::AudioTexture;
use ddi_ui::screens::editor::audio::AudioPicture;

thread_local! {
    static SHOWN: RefCell<Option<AudioPicture>> = const { RefCell::new(None) };
}

/// The editor's picture from now on (`None`: none; the texture goes).
pub fn show(picture: Option<&AudioPicture>) {
    SHOWN.set(picture.cloned());
}

/// The picture on the device, while the editor shows one.
#[derive(Default)]
pub struct EditorAudioHost {
    texture: AudioTexture,
}

impl EditorAudioHost {
    /// Writes the picture shown when it is a new one.
    pub fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, painter: &mut Painter) {
        SHOWN.with_borrow(|p| {
            let picture = p.as_ref().map(|p| (p.serial, p.size, &p.rgba[..]));
            self.texture.show(device, queue, painter, picture);
        });
    }
}
