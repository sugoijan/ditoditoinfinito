//! The field as play draws it, in a panel of the menus: Options' preview
//! (`docs/plans/ui-core.md`, step 6.3). Each frame it plays the demo chart
//! ([`ddi_engine::demo`], looping, autoplayed with a few misses and
//! drops) with the options being chosen and draws the frame as play
//! does, HUD and all, into a texture of its own that the 2D pass draws as
//! an image ([`crate::Painter::set_external_image`], under the source
//! [`SOURCE`]): no copy through the CPU, on every backend.
//!
//! What it follows, from [`FieldPreviewOptions`] (built by the shell from
//! the settings, as play builds its own): the layout (the style chosen:
//! a pump or Dancing☆Onigiri layout draws its own lanes); the scroll
//! (speed, Reverse, Boost, Brake, Wave), Turn and Cut and the
//! simplifications, Hidden, Sudden and Stealth, receptor snap (the
//! player's [`PlayOptions`], played by the demo as by a play); the
//! ruleset (how the demo's hits are judged, and named); the note colors,
//! the field filter and the timing error under the judgement; the
//! background's brightness and the frame it is fitted into, over a plate
//! of our own (the demo has no song background: [`plate`]). The arrows
//! are the shell's to load, the way play loads them, and hand over
//! ([`FieldPreview::set_noteskin`]); without one the built-in arrows
//! draw. A change shows at the next frame; the demo is played again (at
//! the same point of its loop) only when the play options, the layout or
//! the ruleset change.
//!
//! The demo has no speed of its own, so "charts keep their own speed"
//! changes nothing here, as it changes nothing on such a chart in play;
//! the offsets (audio, visual) are not applied: the demo keeps its own
//! time.
//!
//! Frames are drawn as on an sRGB target (linear light), offscreen in a
//! float format, skins' quads blending as StepMania's, and encoded into
//! the plain texture at the end ([`crate::SkinBlending::Encoded`]): the
//! encoded values the 2D pass blends with, as play shows them. Without a
//! float format the device renders and blends (some WebGL2 devices), the
//! frame is drawn straight into the plain texture, blending in encoded
//! values.

use ddi_chart::Layout;
use ddi_engine::demo::Demo;
use ddi_engine::player::PlayOptions;
use ddi_engine::rules::{JudgeNames, RulesetKey, presets};

use crate::background::Backdrop;
use crate::noteskin::{NoteLayer, NoteskinPlay};
use crate::renderer::{Renderer, SkinBlending};
use crate::scene::{ColorScheme, RenderOptions};

/// The texture source the 2D pass draws the preview under.
pub const SOURCE: &str = "field-preview";

/// The format of the texture the preview is drawn into: encoded values,
/// as the 2D pass's images.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The demo's tempo: a common one, so the speed reads as it would on a
/// song.
pub const BPM: f64 = 150.0;

/// A gap between frames longer than this (seconds: a hidden page, a
/// panel not shown) is a pause: the demo goes on from where it was.
const PAUSE: f64 = 0.25;

/// What the preview shows: the options of play that change how the field
/// looks.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldPreviewOptions {
    /// Show the demo's hit effects as autoplay (its timing examples stay).
    pub autoplay_flashes: bool,
    /// A built-in layout's id (the style chosen); another draws
    /// `dance-single`.
    pub layout: String,
    /// The player's play options (`Settings::play_options`): the scroll,
    /// Turn and Cut, the appearance, receptor snap. Offsets and the seed
    /// are not used.
    pub play: PlayOptions,
    /// The ruleset the demo is judged by.
    pub rules: RulesetKey,
    pub note_colors: ColorScheme,
    pub field_filter: f32,
    /// The timing error under the judgement.
    pub show_deltas: bool,
    /// The background's brightness, 0..=1 (0: off).
    pub background: f32,
    /// The shape the background is fitted into (`None`: the whole panel).
    pub frame_aspect: Option<f32>,
}

impl Default for FieldPreviewOptions {
    fn default() -> FieldPreviewOptions {
        FieldPreviewOptions {
            autoplay_flashes: false,
            layout: "dance-single".into(),
            play: PlayOptions::default(),
            rules: presets::itg().key(),
            note_colors: ColorScheme::default(),
            field_filter: 0.0,
            show_deltas: false,
            background: 0.6,
            frame_aspect: None,
        }
    }
}

impl FieldPreviewOptions {
    /// The layout it draws.
    pub fn layout(&self) -> Layout {
        Layout::builtin(&self.layout).unwrap_or_else(Layout::dance_single)
    }

    /// What the demo is played again for.
    fn demo_key(&self) -> (&str, PlayOptions, &RulesetKey) {
        let mut play = self.play;
        play.clock = Default::default();
        play.seed = 0;
        (&self.layout, play, &self.rules)
    }
}

/// The noteskin a preview's layout is drawn with and what it is readied
/// for: its key (`None`: the built-in arrows), the layout, the direction
/// and the values of its parameters (as a play passes them). The demo has
/// no colors of its own.
#[derive(Clone, Debug, PartialEq)]
pub struct SkinWant {
    pub key: Option<String>,
    pub layout: String,
    pub reverse: bool,
    pub params: ddi_noteskin::params::Values,
}

impl SkinWant {
    /// Skin `key` for `options`' layout and direction.
    pub fn new(key: Option<String>, options: &FieldPreviewOptions) -> SkinWant {
        SkinWant {
            key,
            layout: options.layout.clone(),
            reverse: options.play.scroll.reverse,
            params: Default::default(),
        }
    }

    /// The layout the skin is readied for (dance single for one that is
    /// not built in).
    pub fn layout(&self) -> Layout {
        Layout::builtin(&self.layout).unwrap_or_else(Layout::dance_single)
    }

    /// What the skin's Lua is told: the demo's song and chart on
    /// `layout` ([`SkinWant::layout`]), in the direction wanted, with the
    /// parameters' values.
    pub fn env(&self, layout: &Layout) -> ddi_noteskin::lua::Env {
        let env = ddi_engine::demo::song(&self.layout, BPM)
            .and_then(|song| {
                let chart = song.charts.first()?;
                Some(ddi_noteskin::lua::chart_env(
                    &song,
                    chart,
                    layout,
                    self.reverse,
                ))
            })
            .unwrap_or_default();
        ddi_noteskin::lua::Env {
            params: self.params.clone(),
            ..env
        }
    }
}

/// The texture drawn into.
struct Target {
    size: [u32; 2],
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// The preview's state.
pub struct FieldPreview {
    renderer: Renderer,
    options: FieldPreviewOptions,
    layout: Layout,
    demo: Option<Demo>,
    names: JudgeNames,
    noteskin: Option<NoteskinPlay>,
    target: Option<Target>,
    /// The plate behind the field.
    plate: usize,
    /// The demo's time, and the host's at the last frame.
    demo_time: f64,
    last: Option<f64>,
    /// Frames drawn, and textures made (a new one wants registering).
    frames: u64,
    generation: u64,
}

impl FieldPreview {
    /// A preview on `device`, drawn through `float` (a float format the
    /// device renders and blends; `None`: straight into the plain
    /// texture), its HUD in `fonts`.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        float: Option<wgpu::TextureFormat>,
        fonts: &[&'static [u8]],
    ) -> FieldPreview {
        let blending = match float {
            Some(f) => SkinBlending::Encoded(f),
            None => SkinBlending::Scene,
        };
        let mut renderer = Renderer::new(device, queue, FORMAT, blending, fonts);
        let (w, h, rgba) = plate();
        let plate = renderer.add_background_image(device, [w, h], |texture| {
            crate::gpu::write_rgba(queue, texture, [w, h], &rgba);
        });
        let options = FieldPreviewOptions::default();
        let mut preview = FieldPreview {
            renderer,
            layout: options.layout(),
            options,
            demo: None,
            names: presets::itg().names,
            noteskin: None,
            target: None,
            plate,
            demo_time: 0.0,
            last: None,
            frames: 0,
            generation: 0,
        };
        preview.replay();
        preview
    }

    pub fn options(&self) -> &FieldPreviewOptions {
        &self.options
    }

    /// The layout drawn (what a noteskin is loaded for).
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Shows `options` from the next frame; the demo is played again
    /// (from the same point of its loop) when what it plays changed.
    pub fn set_options(&mut self, options: FieldPreviewOptions) {
        if options == self.options {
            return;
        }
        let replay = options.demo_key() != self.options.demo_key();
        self.options = options;
        if replay {
            self.layout = self.options.layout();
            self.replay();
        }
    }

    /// The demo again for the options, at the time it had reached.
    fn replay(&mut self) {
        let (layout, play, rules) = self.options.demo_key();
        let layout = if Layout::builtin(layout).is_some() {
            layout
        } else {
            "dance-single"
        };
        self.demo = Demo::with_options(layout, BPM, play, rules);
        if let Some(d) = &self.demo {
            self.names = d.ruleset().names.clone();
        }
        if let Some(p) = &mut self.noteskin {
            p.restart();
        }
    }

    /// The renderer, for the shell to put a noteskin's textures on (as
    /// play does) inside [`FieldPreview::set_noteskin`].
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    /// Draws the arrows with `play` (loaded for [`FieldPreview::layout`]
    /// and the options' direction; `None`: the built-in arrows) from the
    /// next frame. The last skin's textures go; `upload` puts the new
    /// one's on the renderer and tells `play` their ids.
    pub fn set_noteskin(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        play: Option<NoteskinPlay>,
        upload: impl FnOnce(&mut Renderer, &mut NoteskinPlay),
    ) {
        self.renderer.clear_noteskin_textures();
        self.renderer.set_skins_drawn(device, queue, play.is_some());
        self.noteskin = play.map(|mut p| {
            p.set_srgb(self.renderer.skin_colors_linear());
            upload(&mut self.renderer, &mut p);
            p.restart();
            p
        });
    }

    /// The noteskin drawn, if any.
    pub fn noteskin(&self) -> Option<&NoteskinPlay> {
        self.noteskin.as_ref()
    }

    /// The texture drawn into, once a frame was.
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|t| &t.texture)
    }

    /// Changes each time the texture is made again (a new size): the 2D
    /// pass's image is set again then.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// HUD runs shaped so far.
    pub fn text_shaped(&self) -> u64 {
        self.renderer.text_shaped()
    }

    /// The demo's time, seconds into its play.
    pub fn demo_time(&self) -> f64 {
        self.demo_time
    }

    /// Lets every texture kept at the panel's size go, its own and the
    /// renderer's, while nothing shows it (the shell takes the 2D pass's
    /// image away first: `Painter::set_external_image` with `None`). The
    /// next frame makes them again, under a new
    /// [`FieldPreview::generation`]; the demo goes on from where it was.
    pub fn release_targets(&mut self) {
        self.target = None;
        self.renderer.release_targets();
        self.last = None;
    }

    /// Whether it holds a texture to draw into.
    pub fn has_targets(&self) -> bool {
        self.target.is_some()
    }

    /// Holds the demo at `seconds` (tests): the next frame shows it there.
    pub fn seek(&mut self, seconds: f64) {
        if seconds < self.demo_time {
            self.replay();
        }
        self.demo_time = seconds.max(0.0);
    }

    /// Records a frame of `size` pixels at host time `now` (seconds) into
    /// `encoder`, drawn into the preview's texture (made again for a new
    /// size: [`FieldPreview::generation`]).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        now: f64,
    ) {
        let size = [size[0].max(1), size[1].max(1)];
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let texture = crate::gpu::texture_2d(
                device,
                "ddi-field-preview",
                size,
                FORMAT,
                1,
                // Read back by tests.
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                &[],
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some(Target {
                size,
                texture,
                view,
            });
            self.generation += 1;
        }
        // The demo's time goes on with the host's, a long gap a pause.
        if let Some(last) = self.last {
            let dt = now - last;
            if (0.0..=PAUSE).contains(&dt) {
                self.demo_time += dt;
            }
        }
        self.last = Some(now);
        let reverse = self.options.play.scroll.reverse;
        let Some(demo) = &mut self.demo else {
            return;
        };
        demo.set_autoplay(self.options.autoplay_flashes);
        let step = demo.step(self.demo_time);
        if let Some(p) = &mut self.noteskin {
            p.update(&step.frame, step.events.iter(), reverse);
        }
        let o = &self.options;
        let opts = RenderOptions {
            reverse,
            show_deltas: o.show_deltas,
            note_colors: o.note_colors,
            background: o.background,
            backdrop: Backdrop::image(self.plate),
            frame_aspect: o.frame_aspect,
            field_filter: o.field_filter,
            ..RenderOptions::default()
        };
        let Some(target) = &self.target else {
            return;
        };
        self.renderer.render(
            device,
            queue,
            encoder,
            &target.view,
            None,
            size[0],
            size[1],
            &step.frame,
            &self.layout,
            &self.names,
            opts,
            self.noteskin.as_mut().map(|p| p as &mut dyn NoteLayer),
        );
        self.frames += 1;
    }
}

/// The plate behind the field, standing in for a song's background (the
/// demo has none), 16:9: a dark gradient with soft light bands, so its
/// brightness, its frame and the field filter over it show. Encoded
/// RGBA rows.
pub fn plate() -> (u32, u32, Vec<u8>) {
    let (w, h) = (320u32, 180u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            // Indigo at the top left to teal at the bottom right.
            let t = (u + v) / 2.0;
            let base = [0.16 + 0.04 * t, 0.12 + 0.30 * t, 0.36 + 0.10 * t];
            // Diagonal bands of light, and a glow off centre.
            let band = ((u * 3.0 - v * 1.6) * std::f32::consts::TAU).sin() * 0.5 + 0.5;
            let (dx, dy) = (u - 0.68, v - 0.35);
            let glow = (-(dx * dx * 6.0 + dy * dy * 14.0)).exp();
            for (k, c) in base.iter().enumerate() {
                let tint = [0.55, 0.35, 0.85][k];
                let value = c + 0.10 * band.powi(3) + 0.45 * glow * tint;
                rgba.push((value.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            rgba.push(255);
        }
    }
    (w, h, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_is_played_again_only_for_what_it_plays() {
        let a = FieldPreviewOptions::default();
        let mut b = a.clone();
        b.field_filter = 0.5;
        b.note_colors = ColorScheme::default();
        b.play.clock.visual_offset = 0.03;
        b.play.seed = 9;
        assert_eq!(a.demo_key(), b.demo_key());
        b.play.scroll.reverse = true;
        assert_ne!(a.demo_key(), b.demo_key());
        let mut c = a.clone();
        c.layout = "pump-single".into();
        assert_ne!(a.demo_key(), c.demo_key());
        assert_eq!(c.layout().lane_count(), 5);
        c.layout = "nothing".into();
        assert_eq!(c.layout().lane_count(), 4);
    }

    #[test]
    fn the_plate_is_dark_and_varied() {
        let (w, h, rgba) = plate();
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        let lum: Vec<f32> = rgba
            .chunks(4)
            .map(|p| (f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2])) / 765.0)
            .collect();
        let mean = lum.iter().sum::<f32>() / lum.len() as f32;
        let (lo, hi) = lum
            .iter()
            .fold((1.0f32, 0.0f32), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(mean < 0.45, "{mean}");
        assert!(hi - lo > 0.15, "{lo} {hi}");
    }
}
