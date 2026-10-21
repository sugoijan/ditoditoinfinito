//! A song's scripted backgrounds and foregrounds in play
//! (`docs/plans/scripted-backgrounds.md`): the runtime
//! (`ddi_session::scripted`), the images its scripts load (decoded once
//! per song by the shell, as a work's layers' are, [`StageContent`]),
//! their textures registered as a noteskin's are (so again after a skin
//! replaces them), and the quads each frame (`ddi_render::scripted`).

use std::collections::{HashMap, HashSet};

use ddi_actor::text::{RunMetrics, TextMeasure, TextStyle};

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
    /// Their texts drawn (white, by the shell), by text: the drawing, its
    /// size in StepMania's pixels and its registered texture.
    texts: HashMap<String, (C, [f32; 2], Option<u32>)>,
    /// Texts asked of the shell, drawn or not yet.
    asked: HashSet<String>,
}

impl<C> Default for ScriptView<C> {
    fn default() -> Self {
        ScriptView {
            scripts: None,
            screen: [854.0, 480.0],
            images: HashMap::new(),
            textures: Vec::new(),
            texts: HashMap::new(),
            asked: HashSet::new(),
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
        self.texts.clear();
        self.asked.clear();
    }

    /// Measures the scripts' text (after [`ScriptView::frame`], before
    /// [`ScriptView::quads`]): the texts the shell has not been asked to
    /// draw yet, to draw white at `ddi_session::scripted::TEXT_SIZE`
    /// ([`ScriptView::add_text`]). Until drawn a text is measured as a
    /// guess and not shown.
    pub fn measure(&mut self) -> Vec<String> {
        let Some(scripts) = self.scripts.as_mut() else {
            return Vec::new();
        };
        let mut m = Measured {
            texts: &self.texts,
            missing: Vec::new(),
        };
        for tree in scripts.trees_mut() {
            tree.measure(&mut m);
        }
        let mut wanted = Vec::new();
        for text in m.missing {
            if self.asked.len() >= ddi_session::scripted::MAX_TEXTS {
                break;
            }
            if self.asked.insert(text.clone()) {
                wanted.push(text);
            }
        }
        wanted
    }

    /// A text the shell drew, `size` in StepMania's pixels: measured anew.
    pub fn add_text(&mut self, text: String, content: C, size: [f32; 2]) {
        self.texts.insert(text, (content, size, None));
        if let Some(scripts) = self.scripts.as_mut() {
            for tree in scripts.trees_mut() {
                tree.forget_metrics();
            }
        }
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

    /// The sounds the scripts played since last asked
    /// ([`Scripts::take_sounds`]).
    pub fn take_sounds(&mut self) -> Vec<String> {
        self.scripts
            .as_mut()
            .map_or(Vec::new(), Scripts::take_sounds)
    }

    /// The sound files the scripts may play ([`Scripts::sound_files`]).
    pub fn sound_files(&self) -> Vec<String> {
        self.scripts
            .as_ref()
            .map_or(Vec::new(), Scripts::sound_files)
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
        for t in self.texts.values_mut() {
            t.2 = None;
        }
    }

    /// Copies the scripts' images to the GPU, once after each clearing.
    pub fn upload(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(scripts) = &self.scripts else {
            return;
        };
        for (content, _, id) in self.texts.values_mut().filter(|t| t.2.is_none()) {
            let [w, h] = content.size();
            let pixels = [w.max(1), h.max(1)];
            *id = Some(
                renderer.add_noteskin_image(device, queue, pixels, false, |texture| {
                    content.write(queue, texture, pixels);
                }),
            );
        }
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
        let texts = |text: &str| {
            let (_, size, id) = self.texts.get(text)?;
            Some(((*id)?, *size))
        };
        ddi_render::scripted::quads(draws, frame, self.screen, &self.textures, &texts, srgb)
    }
}

/// Measures runs by the shell's drawings of them; a run not drawn yet is
/// guessed ([`ddi_actor::text::FixedMeasure`]) and noted.
struct Measured<'a, C> {
    texts: &'a HashMap<String, (C, [f32; 2], Option<u32>)>,
    missing: Vec<String>,
}

impl<C> TextMeasure for Measured<'_, C> {
    fn measure(&mut self, text: &str, style: &TextStyle) -> RunMetrics {
        let Some((_, [w, h], _)) = self.texts.get(text) else {
            self.missing.push(text.to_string());
            return ddi_actor::text::FixedMeasure.measure(text, style);
        };
        // The drawing's top is the line box's (the shells draw from the
        // top, a line 1.2 sizes apart).
        let baseline = style.line_height() * 0.8;
        RunMetrics {
            advance: *w,
            ink: [0.0, -baseline, *w, h - baseline],
            baseline,
        }
    }
}
