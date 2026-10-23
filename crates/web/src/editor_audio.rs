//! The chart editor's audio view on the web (`docs/plans/editor.md`,
//! "The audio view"): the picture the editor screen painted, handed
//! over by its host ([`show`]), written by the editor page's frame hook
//! ([`hook`], beside the field's) into the texture the 2D pass shows as
//! `editor-audio`, only when it is a new one
//! (`ddi_render::editor_audio`); written again on a new device.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_render::editor_audio::AudioTexture;
use ddi_ui::screens::editor::audio::AudioPicture;

use crate::components::ui_dev::FrameHook;

thread_local! {
    static SHOWN: RefCell<Option<AudioPicture>> = const { RefCell::new(None) };
}

/// The editor's picture from now on (`None`: none; the texture goes).
pub(crate) fn show(picture: Option<&AudioPicture>) {
    SHOWN.set(picture.cloned());
}

/// `inner` (the field's hook), then the picture written when it is a
/// new one.
pub(crate) fn hook(mut inner: FrameHook) -> FrameHook {
    let slot: Rc<RefCell<Option<(u64, AudioTexture)>>> = Rc::default();
    Box::new(move |gfx, encoder, ui, time| {
        let busy = inner(gfx, encoder, ui, time);
        let mut guard = slot.borrow_mut();
        if guard.as_ref().is_none_or(|(g, _)| *g != gfx.generation) {
            *guard = Some((gfx.generation, AudioTexture::default()));
        }
        if let Some((_, texture)) = guard.as_mut() {
            SHOWN.with_borrow(|p| {
                let picture = p.as_ref().map(|p| (p.serial, p.size, &p.rgba[..]));
                crate::web::screen::with_painter(gfx, |painter| {
                    texture.show(&gfx.device, &gfx.queue, painter, picture);
                });
            });
        }
        busy
    })
}
