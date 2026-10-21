//! A song's scripted backgrounds and foregrounds in play
//! (`docs/plans/scripted-backgrounds.md`): the runtime
//! (`ddi_session::scripted`), the images its scripts load (decoded once
//! per song by the shell, as a work's layers' are, [`StageContent`]),
//! their textures registered as a noteskin's are (so again after a skin
//! replaces them), and the quads each frame (`ddi_render::scripted`).

use std::collections::HashMap;

use ddi_library::backgrounds::{BgSegment, LAYERS};
use ddi_noteskin::build::TextureKey;
use ddi_render::Renderer;
use ddi_render::textured::TexQuad;
use ddi_session::scripted::Scripts;

use crate::stage::StageContent;

/// The scripts of a play: what they load, their textures, their stop.
pub struct ScriptView<C> {
    scripts: Option<Scripts>,
    /// StepMania's screen the scripts were told.
    screen: [f32; 2],
    /// Images decoded, by path relative to the song's folder, lower case.
    images: HashMap<String, C>,
    /// The scripts' textures' registered ids, by the ids their sprites
    /// carry; empty since the textures were last cleared.
    textures: Vec<Option<u32>>,
}

impl<C> Default for ScriptView<C> {
    fn default() -> Self {
        ScriptView {
            scripts: None,
            screen: [854.0, 480.0],
            images: HashMap::new(),
            textures: Vec::new(),
        }
    }
}

impl<C: StageContent> ScriptView<C> {
    /// The play's scripts (`None`: none), told StepMania's `screen`, with
    /// the images they load (by path relative to the song's folder).
    pub fn set(&mut self, scripts: Option<Scripts>, screen: [f64; 2], images: Vec<(String, C)>) {
        self.scripts = scripts;
        self.screen = screen.map(|v| v as f32);
        self.images = images
            .into_iter()
            .map(|(p, c)| (p.to_lowercase(), c))
            .collect();
        self.textures.clear();
    }

    /// A new play of the same song ([`Scripts::restart`]).
    pub fn restart(&mut self) {
        if let Some(s) = self.scripts.as_mut() {
            s.restart();
        }
    }

    /// The foregrounds' times for the play ([`Scripts::retime`]).
    pub fn retime(&mut self, foregrounds: &[ddi_library::backgrounds::Foreground]) {
        if let Some(s) = self.scripts.as_mut() {
            s.retime(foregrounds);
        }
    }

    /// Whether scripts draw in this play (they have not stopped).
    pub fn running(&self) -> bool {
        self.scripts.as_ref().is_some_and(|s| s.stopped().is_none())
    }

    /// What the scripts noted while loading.
    pub fn notes(&self) -> &[String] {
        self.scripts.as_ref().map_or(&[], |s| s.notes())
    }

    /// Fuel the scripts spent in the last frame.
    pub fn frame_fuel(&self) -> u64 {
        self.scripts.as_ref().map_or(0, Scripts::frame_fuel)
    }

    /// The renderer's skin textures were cleared, and these with them.
    pub fn forget_textures(&mut self) {
        self.textures.clear();
    }

    /// Copies the scripts' images to the GPU, once after each clearing.
    pub fn upload(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(scripts) = &self.scripts else {
            return;
        };
        let keys = scripts.textures().keys();
        if !self.textures.is_empty() || keys.is_empty() {
            return;
        }
        self.textures = keys
            .iter()
            .map(|key| {
                let TextureKey::Skin { path, .. } = key else {
                    return None;
                };
                let content = self.images.get(&path.to_lowercase())?;
                let [w, h] = content.size();
                let pixels = [w.max(1), h.max(1)];
                Some(
                    renderer.add_noteskin_image(device, queue, pixels, false, |texture| {
                        content.write(queue, texture, pixels);
                    }),
                )
            })
            .collect();
    }

    /// One frame at song second `t` (beat `beat`): see
    /// [`Scripts::frame`]. Why they stopped, in the frame they did.
    pub fn frame(
        &mut self,
        t: f64,
        beat: f64,
        layers: &[Vec<BgSegment>; LAYERS],
    ) -> Option<String> {
        self.scripts.as_mut()?.frame(t, beat, layers)
    }

    /// The background and foreground quads at song second `t`, in the
    /// background `frame` (`x, y, width, height` in target pixels).
    pub fn quads(
        &mut self,
        t: f64,
        layers: &[Vec<BgSegment>; LAYERS],
        frame: [f32; 4],
        srgb: bool,
    ) -> (Vec<TexQuad>, Vec<TexQuad>) {
        let Some(scripts) = self.scripts.as_mut() else {
            return (Vec::new(), Vec::new());
        };
        let draws = scripts.draws(t, layers);
        ddi_render::scripted::quads(draws, frame, self.screen, &self.textures, srgb)
    }
}
