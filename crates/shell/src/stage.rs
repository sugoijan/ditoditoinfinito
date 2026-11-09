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
use ddi_render::noteskin::NoteskinPlay;
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

    /// Copies it to a new texture registered as a noteskin image's (a
    /// model's with `mipmaps`, as StepMania's are), and says which.
    fn upload(
        &self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mipmaps: bool,
    ) -> u32 {
        let [w, h] = self.size();
        let pixels = [w.max(1), h.max(1)];
        renderer.add_noteskin_image(device, queue, pixels, mipmaps, |texture| {
            self.write(queue, texture, pixels);
        })
    }
}

impl<C: StageContent> StageContent for &C {
    fn size(&self) -> [u32; 2] {
        (**self).size()
    }

    fn write(&self, queue: &wgpu::Queue, texture: &wgpu::Texture, size: [u32; 2]) {
        (**self).write(queue, texture, size);
    }
}

/// Gives the renderer `play`'s textures: the built-in art, and the decoded
/// `images` (in texture order; none for built-in art), a model's with
/// mipmaps.
pub fn upload_noteskin<C: StageContent>(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    play: &mut NoteskinPlay,
    images: impl IntoIterator<Item = Option<C>>,
) {
    let mut images = images.into_iter();
    let keys = play.texture_keys().to_vec();
    for (i, key) in keys.iter().enumerate() {
        let id = match (key, images.next().flatten()) {
            (ddi_noteskin::build::TextureKey::BuiltIn(name), _) => {
                renderer.add_builtin_art(device, queue, name)
            }
            (_, Some(image)) => image.upload(renderer, device, queue, play.texture_mipmaps(i)),
            (_, None) => continue,
        };
        play.set_texture(i, id);
    }
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
    /// A content decoded is not on the GPU yet.
    pending: bool,
}

impl<C> Default for StageView<C> {
    fn default() -> Self {
        StageView {
            layers: StageLayers::default(),
            contents: HashMap::new(),
            textures: HashMap::new(),
            pending: false,
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
        self.pending = true;
    }

    /// The renderer's skin textures were cleared, and these with them.
    pub fn forget_textures(&mut self) {
        self.textures.clear();
        self.pending = !self.contents.is_empty();
    }

    /// Whether a decoded content is not on the GPU yet.
    pub fn pending(&self) -> bool {
        self.pending
    }

    /// Copies the decoded contents not on the GPU yet.
    pub fn upload(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !std::mem::take(&mut self.pending) {
            return;
        }
        for (source, (content, size)) in &self.contents {
            if self.textures.contains_key(source) {
                continue;
            }
            let id = content.upload(renderer, device, queue, false);
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
