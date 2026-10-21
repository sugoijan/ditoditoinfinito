//! A Dancing☆Onigiri work's back and mask layers in play
//! (`docs/plans/overlays.md`): the contents a song's layers show (its
//! images and its texts, decoded once per song by the shell: a retry keeps
//! them), their textures, registered as a noteskin's are (so again after a
//! skin replaces them), and the play's timeline (`ddi_session::stage`).
//!
//! Each shell brings its content ([`StageContent`]): the web an
//! `ImageBitmap` copied to its texture by the browser, desktop RGBA rows
//! written to it.

use std::collections::HashMap;

use ddi_engine::sprites::{SpriteSource, SpriteTrack};
use ddi_render::Renderer;
use ddi_render::stage::{self, StageTexture};
use ddi_render::textured::TexQuad;
use ddi_session::stage::StageLayers;

/// A content decoded by a shell: its pixels' size, and how they are
/// copied to a texture.
pub trait StageContent {
    /// Its pixels' size, `[width, height]` (a side of 0 is taken as 1).
    fn size(&self) -> [u32; 2];

    /// Copies it into `texture`'s first level, `size` pixels (its size,
    /// each side at least 1).
    fn write(&self, queue: &wgpu::Queue, texture: &wgpu::Texture, size: [u32; 2]);
}

/// The layers in a play: the contents decoded, their textures, the
/// play's timeline.
pub struct StageView<C> {
    /// The play's layers; none between plays or when not shown.
    layers: StageLayers,
    /// Contents decoded and their size in the stage's px.
    contents: HashMap<SpriteSource, (C, (f32, f32))>,
    /// Contents copied to the GPU since the textures were last cleared.
    textures: HashMap<SpriteSource, StageTexture>,
}

impl<C> Default for StageView<C> {
    fn default() -> Self {
        StageView {
            layers: StageLayers::default(),
            contents: HashMap::new(),
            textures: HashMap::new(),
        }
    }
}

impl<C: StageContent> StageView<C> {
    /// The layers of the play about to start (`None`: none shown).
    pub fn set_track(&mut self, track: Option<SpriteTrack>) {
        self.layers.set_track(track);
    }

    /// Whether a play's layers are shown.
    pub fn shown(&self) -> bool {
        self.layers.shown()
    }

    /// A content decoded (an image, or text drawn), `size` in the stage's
    /// px; the one it replaces, if any, is dropped.
    pub fn add(&mut self, source: SpriteSource, content: C, size: (f32, f32)) {
        self.textures.remove(&source);
        self.contents.insert(source, (content, size));
    }

    /// The renderer's skin textures were cleared, and these with them.
    pub fn forget_textures(&mut self) {
        self.textures.clear();
    }

    /// Whether a decoded content is not on the GPU yet.
    pub fn pending(&self) -> bool {
        self.contents.keys().any(|s| !self.textures.contains_key(s))
    }

    /// Copies the decoded contents not on the GPU yet.
    pub fn upload(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        for (source, (content, size)) in &self.contents {
            if self.textures.contains_key(source) {
                continue;
            }
            let [w, h] = content.size();
            let pixels = [w.max(1), h.max(1)];
            let id = renderer.add_noteskin_image(device, queue, pixels, false, |texture| {
                content.write(queue, texture, pixels);
            });
            self.textures
                .insert(source.clone(), StageTexture { id, size: *size });
        }
    }

    /// The back and mask quads at song second `t`, on a target `height` px
    /// tall with the field centered at `center_x`.
    pub fn quads(
        &self,
        t: f64,
        center_x: f32,
        height: f32,
        srgb: bool,
    ) -> (Vec<TexQuad>, Vec<TexQuad>) {
        let Some((sprites, size)) = self.layers.at(t) else {
            return (Vec::new(), Vec::new());
        };
        stage::quads(
            &sprites,
            size,
            center_x,
            height,
            |s| self.textures.get(s).copied(),
            srgb,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    /// A content that counts its drops.
    struct Counted(Rc<Cell<u32>>);

    impl StageContent for Counted {
        fn size(&self) -> [u32; 2] {
            [1, 1]
        }
        fn write(&self, _: &wgpu::Queue, _: &wgpu::Texture, _: [u32; 2]) {}
    }

    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn a_content_replaced_or_left_is_dropped() {
        let dropped = Rc::new(Cell::new(0));
        let source = SpriteSource::Image(Rc::from("a.png"));
        let mut view = StageView::default();
        assert!(!view.pending() && !view.shown());
        view.add(source.clone(), Counted(dropped.clone()), (1.0, 1.0));
        assert!(view.pending());
        view.add(source, Counted(dropped.clone()), (2.0, 2.0));
        assert_eq!(dropped.get(), 1);
        drop(view);
        assert_eq!(dropped.get(), 2);
    }

    #[test]
    fn no_track_draws_nothing() {
        let mut view = StageView::<Counted>::default();
        view.set_track(None);
        let (back, mask) = view.quads(0.0, 100.0, 480.0, false);
        assert!(back.is_empty() && mask.is_empty());
    }
}
