//! A Dancing☆Onigiri work's back and mask layers in play
//! (`docs/plans/overlays.md`): the images and text a song's layers show,
//! decoded once per song (a retry keeps them), their textures, registered
//! as a noteskin's are (and again after a skin replaces them), and the
//! play's timeline (`ddi_session::stage`).

use std::collections::HashMap;

use ddi_engine::sprites::{SpriteSource, SpriteTrack};
use ddi_render::Renderer;
use ddi_render::stage::{self, StageTexture};
use ddi_render::textured::TexQuad;
use ddi_session::stage::StageLayers;

#[derive(Default)]
pub(crate) struct StageView {
    /// The play's layers; none between plays or when not shown.
    layers: StageLayers,
    /// Decoded contents and their size in the stage's px.
    bitmaps: HashMap<SpriteSource, (web_sys::ImageBitmap, (f32, f32))>,
    /// Contents copied to the GPU since the textures were last cleared.
    textures: HashMap<SpriteSource, StageTexture>,
}

impl StageView {
    /// The layers of the play about to start (`None`: none shown).
    pub(crate) fn set_track(&mut self, track: Option<SpriteTrack>) {
        self.layers.set_track(track);
    }

    /// Whether a play's layers are shown.
    pub(crate) fn shown(&self) -> bool {
        self.layers.shown()
    }

    /// A content decoded (an image, or text drawn), `size` in the stage's
    /// px.
    pub(crate) fn add(
        &mut self,
        source: SpriteSource,
        bitmap: web_sys::ImageBitmap,
        size: (f32, f32),
    ) {
        if let Some((old, _)) = self.bitmaps.insert(source.clone(), (bitmap, size)) {
            old.close();
        }
        self.textures.remove(&source);
    }

    /// The renderer's skin textures were cleared, and these with them.
    pub(crate) fn forget_textures(&mut self) {
        self.textures.clear();
    }

    /// Whether a decoded content is not on the GPU yet.
    pub(crate) fn pending(&self) -> bool {
        self.bitmaps.keys().any(|s| !self.textures.contains_key(s))
    }

    /// Copies the decoded contents not on the GPU yet.
    pub(crate) fn upload(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        for (source, (bitmap, size)) in &self.bitmaps {
            if self.textures.contains_key(source) {
                continue;
            }
            let (w, h) = (bitmap.width().max(1), bitmap.height().max(1));
            let texture = renderer.create_noteskin_texture(device, w, h, false);
            queue.copy_external_image_to_texture(
                &wgpu::CopyExternalImageSourceInfo {
                    source: wgpu::ExternalImageSource::ImageBitmap(bitmap.clone()),
                    origin: wgpu::Origin2d::ZERO,
                    flip_y: false,
                },
                wgpu::CopyExternalImageDestInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                    color_space: wgpu::PredefinedColorSpace::Srgb,
                    premultiplied_alpha: false,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            let id = renderer.add_noteskin_texture(device, &texture);
            self.textures
                .insert(source.clone(), StageTexture { id, size: *size });
        }
    }

    /// The back and mask quads at song second `t`, on a target `height` px
    /// tall with the field centred at `center_x`.
    pub(crate) fn quads(
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

impl Drop for StageView {
    fn drop(&mut self) {
        for (bitmap, _) in self.bitmaps.values() {
            bitmap.close();
        }
    }
}
