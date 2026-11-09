//! A play's generated background (`docs/plans/phase-8-gaps.md`, step 5),
//! for both shells: which look a song gets under the settings
//! ([`wanted`]), and the texture it is drawn into each frame it shows,
//! registered with the backdrop ([`Generated`]). The music's analysis
//! comes from each shell its own way (desktop on a thread, the web in
//! slices; [`Generated::set_series`]); until it is in, the look runs on
//! the beat alone. The kaleidoscope folds the song's cover (its jacket,
//! else its banner: [`cover`]) where it has no background; the start
//! prompt shows the look's first frame, held still ([`Still`],
//! [`Stills`]).

use std::rc::Rc;

use ddi_library::backgrounds::BgImage;
use ddi_library::generative::{Look, Palette, Series, Signals, stands_in};
use ddi_library::manifest::ManifestEntry;
use ddi_library::path::RelPathBuf;
use ddi_render::generative::{FORMAT, Frame, GenerativePipeline};
use ddi_render::{Painter, RenderOptions, Renderer, TextureId, TextureKey};
use ddi_session::movies::{BackdropTextures, Generated as InBackdrop};
use ddi_ui::screens::prompt::PromptPicture;
use ddi_ui::state::settings::Settings;

/// The background brightness the looks are drawn at in full, as the
/// study drew them: the setting's default. Under it they dim with it.
const FULL_AT: f32 = 0.4;

/// What a play's generated background is: its look and colors, and
/// whether it goes over the song's own picture too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Want {
    pub look: Look,
    pub palette: Palette,
    pub over: bool,
}

/// The generated background `song` gets under `settings`: none when it
/// is turned off, with the backgrounds off, in calibration
/// (`calibration`: nothing may cue the player) or for a
/// Dancing☆Onigiri work (decision 5).
pub fn wanted(settings: &Settings, song: &ddi_chart::Song, calibration: bool) -> Option<Want> {
    let look = settings.generated_background.look()?;
    let shown = settings.bg_brightness > 0.0 && !calibration;
    (shown && stands_in(song)).then_some(Want {
        look,
        palette: settings.generated_palette,
        over: settings.generated_over_pictures,
    })
}

/// Whether a song with a background picture (`has_picture`) needs its
/// music analyzed for `want`: when the look may show, standing in for no
/// picture or going over it.
pub fn needs_music(want: Option<Want>, has_picture: bool) -> bool {
    want.is_some_and(|w| w.over || !has_picture)
}

/// The picture the kaleidoscope folds where a song has no background of
/// its own: its jacket, else its banner.
pub fn cover(entry: &ManifestEntry) -> Option<&RelPathBuf> {
    entry.jacket.as_ref().or(entry.banner.as_ref())
}

/// Whether a song with a background picture (`has_picture`) needs its
/// [`cover`] read for `want`: for the kaleidoscope standing in.
pub fn needs_cover(want: Option<Want>, has_picture: bool) -> bool {
    want.is_some_and(|w| w.look == Look::Kaleidoscope && !has_picture)
}

/// The picture a look is drawn with, among the renderer's backgrounds:
/// the song's own (`song`) when it goes over it, else the cover
/// (`cover`) the kaleidoscope folds; `None`, none (the kaleidoscope's
/// glass).
pub fn folded(want: Want, song: Option<usize>, cover: Option<usize>) -> Option<usize> {
    match song.filter(|_| want.over) {
        Some(id) => Some(id),
        None if want.look == Look::Kaleidoscope => cover,
        None => None,
    }
}

/// The renderer's id of background image `what` among `images` (a
/// shell's list of the song's images up), once up.
pub fn picture_id(images: &[(BgImage, usize)], what: &BgImage) -> Option<usize> {
    images.iter().find(|(w, _)| w == what).map(|(_, id)| *id)
}

/// How bright a look is drawn under background brightness `brightness`.
pub fn light(brightness: f32) -> f32 {
    (brightness / FULL_AT).clamp(0.0, 1.0)
}

/// The texture's size for a frame of `frame` pixels (the background
/// frame's, or the screen's): the frame's own, its shape kept within
/// `most` pixels a side (the device's largest texture; WebGL2 may allow
/// less than a 4K screen).
pub fn texture_size(frame: [f32; 2], most: u32) -> [u32; 2] {
    let [w, h] = frame.map(|v| v.max(1.0));
    let k = (most.max(1) as f32 / w.max(h)).min(1.0);
    [
        ((w * k).round() as u32).clamp(1, most.max(1)),
        ((h * k).round() as u32).clamp(1, most.max(1)),
    ]
}

/// A play's generated background: its pass, the texture it draws into
/// and its id among the backdrop's images, and the music's signals.
pub struct Generated {
    pub want: Want,
    pipeline: Option<GenerativePipeline>,
    texture: Option<(wgpu::Texture, wgpu::TextureView, [u32; 2])>,
    id: Option<usize>,
    series: Option<Rc<Series>>,
    /// How many analyses it was given: which one a held frame shows.
    analyses: u32,
    /// The song picture bound to the pass (its id), over it.
    picture: Option<usize>,
    /// Frames drawn.
    drawn: u64,
    /// What the texture holds while motion is reduced (the picture held
    /// still): drawing it again would change nothing.
    held: Option<Held>,
}

/// A held frame's inputs: the wanted look, the analysis, the picture,
/// the brightness and the texture's size.
type Held = (Want, u32, Option<usize>, u32, [u32; 2]);

/// What the debug state says of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Debug {
    pub look: String,
    pub palette: String,
    pub over: bool,
    /// The analysis' frames, once in.
    pub analyzed: Option<usize>,
    pub drawn: u64,
    pub size: [u32; 2],
}

impl Generated {
    /// Makes its pass ahead of its first frame (behind the start prompt).
    pub fn warm(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.pipeline
            .get_or_insert_with(|| GenerativePipeline::new(device, queue));
    }

    pub fn new(want: Want) -> Generated {
        Generated {
            want,
            pipeline: None,
            texture: None,
            id: None,
            series: None,
            analyses: 0,
            picture: None,
            drawn: 0,
            held: None,
        }
    }

    /// The music's signals, analyzed.
    pub fn set_series(&mut self, series: Rc<Series>) {
        self.series = Some(series);
        self.analyses += 1;
    }

    pub fn analyzed(&self) -> bool {
        self.series.is_some()
    }

    /// Its texture for a background frame of `frame` pixels, made (or
    /// made again at a new size) and registered with the renderer: what
    /// the backdrop's list gets ([`ddi_session::movies::BackdropTextures::set_generated`]).
    fn ensure(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        frame: [f32; 2],
    ) -> InBackdrop {
        let size = texture_size(frame, device.limits().max_texture_dimension_2d);
        if self.texture.as_ref().is_none_or(|t| t.2 != size) {
            let texture = renderer.create_generated_texture(device, size[0], size[1]);
            debug_assert_eq!(texture.format(), FORMAT);
            match self.id {
                Some(id) => renderer.replace_generated(device, id, &texture),
                None => self.id = Some(renderer.add_generated(device, &texture)),
            }
            let view = texture.create_view(&Default::default());
            self.texture = Some((texture, view, size));
            self.held = None;
        }
        InBackdrop {
            id: self.id.expect("registered"),
            over: self.want.over,
        }
    }

    /// A play's frame before its backdrop is chosen: while the
    /// backgrounds show (`render`'s brightness), its texture at the
    /// background frame's size (on a screen of `size` pixels) in
    /// `textures`' list ([`Generated::ensure`]).
    pub fn prepare(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        textures: &mut BackdropTextures,
        size: [u32; 2],
        render: &RenderOptions,
    ) {
        if render.background <= 0.0 {
            return;
        }
        let [w, h] = size.map(|v| v as f32);
        let frame = ddi_render::background::frame_rect(w, h, render.frame_aspect)
            .map_or([w, h], |r| [r[2], r[3]]);
        let entry = self.ensure(renderer, device, frame);
        textures.set_generated(Some(entry));
    }

    /// A play's frame once its backdrop is chosen (`render`'s): drawn at
    /// song second `t` and beat `beat` when the backdrop shows it, over
    /// the song's picture or folding its cover, among `images` (the
    /// shell's list of the song's images up).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_shown(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        render: &RenderOptions,
        images: &[(BgImage, usize)],
        (t, beat): (f64, f64),
    ) {
        if !self.id.is_some_and(|id| shown(&render.backdrop, id)) {
            return;
        }
        let pictures = (
            picture_id(images, &BgImage::Song),
            picture_id(images, &BgImage::Cover),
        );
        self.draw(
            renderer,
            device,
            queue,
            encoder,
            (t, beat),
            render.background,
            render.reduced_motion,
            pictures,
        );
    }

    /// The signals at song second `t`, beat `beat`: held at the start
    /// for reduced motion.
    fn signals(&self, t: f64, beat: f64, reduced: bool) -> Signals {
        let (t, beat) = if reduced { (0.0, 0.0) } else { (t, beat) };
        match &self.series {
            Some(s) => s.at(t, beat),
            None => Signals::beat_only(t, beat),
        }
    }

    /// Draws the frame at song second `t` and beat `beat` into the
    /// texture, under background brightness `brightness`; `pictures` the
    /// song's own picture's and its cover's ids among the renderer's
    /// backgrounds, those it has ([`folded`]). Call after [`ensure`] and
    /// only when the backdrop shows it.
    ///
    /// [`ensure`]: Generated::ensure
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        (t, beat): (f64, f64),
        brightness: f32,
        reduced: bool,
        (song, cover): (Option<usize>, Option<usize>),
    ) {
        let signals = self.signals(t, beat, reduced);
        let Some((_, view, size)) = self.texture.as_ref() else {
            return;
        };
        let pipeline = self
            .pipeline
            .get_or_insert_with(|| GenerativePipeline::new(device, queue));
        let picture = folded(self.want, song, cover);
        if picture != self.picture {
            pipeline.set_picture(
                device,
                picture.and_then(|id| renderer.background_picture(id)),
            );
            self.picture = picture;
        }
        let held = reduced.then_some((
            self.want,
            self.analyses,
            picture,
            brightness.to_bits(),
            *size,
        ));
        if held.is_some() && held == self.held {
            return;
        }
        self.held = held;
        let frame = Frame {
            look: self.want.look,
            palette: self.want.palette,
            signals: &signals,
            light: light(brightness),
            picture: pipeline.has_picture(),
            picture_brightness: brightness,
        };
        pipeline.draw(queue, encoder, view, *size, &frame);
        // The backdrop's blurred copies follow it, as a movie's.
        renderer.background_texture_changed();
        self.drawn += 1;
    }

    /// What the start prompt shows of it, the song having no picture of
    /// its own: the frame at song second `t` and beat `beat` (the play's
    /// first), as [`Generated::draw`] would draw it then, for a frame of
    /// shape `aspect` (width over height); the cover folded found among
    /// `images` (the shell's list of the song's images up), once up.
    /// `None` while the cover it folds is read but not up.
    pub fn still(
        &self,
        renderer: &Renderer,
        images: &[(BgImage, usize)],
        (t, beat): (f64, f64),
        brightness: f32,
        reduced: bool,
        aspect: f32,
    ) -> Option<Still> {
        let cover = folded(self.want, None, picture_id(images, &BgImage::Cover));
        let picture = match cover {
            Some(id) => Some(renderer.background_picture(id)?),
            None => None,
        };
        Some(Still {
            want: self.want,
            signals: self.signals(t, beat, reduced),
            light: light(brightness),
            aspect,
            picture: picture.map(|(view, size, linear)| (view.clone(), size, linear)),
            cover,
        })
    }

    pub fn debug(&self) -> Debug {
        Debug {
            look: format!("{:?}", self.want.look),
            palette: format!("{:?}", self.want.palette),
            over: self.want.over,
            analyzed: self.series.as_ref().map(|s| s.frames()),
            drawn: self.drawn,
            size: self.texture.as_ref().map_or([0; 2], |t| t.2),
        }
    }
}

/// A generated background's frame held still: what the start prompt
/// shows for a song without a picture, as its picture (the maintainer,
/// 2026-11-06), so the play's entrance carries on from it as from a
/// picture ([`Generated::still`]).
#[derive(Clone, Debug)]
pub struct Still {
    want: Want,
    signals: Signals,
    light: f32,
    /// The frame's shape, width over height.
    pub aspect: f32,
    /// The cover folded: its view, size and whether it samples as
    /// linear light.
    picture: Option<(wgpu::TextureView, [u32; 2], bool)>,
    /// The cover's id, telling one picture from another.
    cover: Option<usize>,
}

impl Still {
    /// Whether it draws the same picture as `other`.
    fn same(&self, other: &Still) -> bool {
        self.want == other.want
            && self.signals == other.signals
            && self.light == other.light
            && self.aspect == other.aspect
            && self.cover == other.cover
    }
}

/// The start prompt's picture: the song's own (`song`: its texture
/// source and shape) at the backgrounds' brightness `brightness`, else
/// its generated background's [`Still`] (`still`: the source it is
/// drawn under and its shape) at full brightness, the still being drawn
/// under the backgrounds' brightness already ([`light`]); none with the
/// backgrounds off. `frame` and `filter` as the prompt takes them.
pub fn prompt_picture(
    song: Option<(String, f32)>,
    still: Option<(String, f32)>,
    brightness: f32,
    frame: Option<f32>,
    filter: Option<([f32; 2], f32)>,
) -> Option<PromptPicture> {
    if brightness <= 0.0 {
        return None;
    }
    let (source, aspect, brightness) = match (song, still) {
        (Some((source, aspect)), _) => (source, aspect, brightness),
        (None, Some((source, aspect))) => (source, aspect, 1.0),
        (None, None) => return None,
    };
    Some(PromptPicture {
        source,
        aspect,
        frame,
        brightness,
        filter,
    })
}

/// A texture source of its own for a prompt's still (the painter keeps
/// what a source showed: each song's is new).
pub fn still_source() -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("generated-still:{n}")
}

/// A texture's size of shape `aspect` (width over height) whose longer
/// side is `side` pixels.
pub fn still_size(aspect: f32, side: u32) -> [u32; 2] {
    let side = side.max(1) as f32;
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    let [w, h] = if aspect >= 1.0 {
        [side, side / aspect]
    } else {
        [side * aspect, side]
    };
    [w.round().max(1.0) as u32, h.round().max(1.0) as u32]
}

/// Draws the prompt's [`Still`] for the 2D pass: the painter asks for its
/// source as for any image (sharp at the frame's size, blurred at the
/// backdrop's), and each is drawn into the painter's texture at the size
/// asked rather than read from a file; the painter then filters it as an
/// image it read. Drawn again where the still changes (the music's
/// analysis or the cover come in), forgotten when another song's comes.
#[derive(Default)]
pub struct Stills {
    /// The texture source it is drawn under, and what.
    source: Option<String>,
    still: Option<Still>,
    pipeline: Option<GenerativePipeline>,
    /// The painter's textures drawn: each id and its longer side.
    filled: Vec<(TextureId, u32)>,
    /// Sources gone, their textures to let go.
    stale: Vec<String>,
    /// The still changed since its textures were drawn.
    changed: bool,
}

impl Stills {
    /// The still drawn under `source` from now on (`None`: none).
    pub fn set(&mut self, source: Option<String>, still: Option<Still>) {
        if source != self.source {
            self.stale.extend(self.source.take());
            self.filled.clear();
            self.source = source;
        }
        let changed = match (&self.still, &still) {
            (Some(a), Some(b)) => !a.same(b),
            (None, None) => false,
            _ => true,
        };
        self.changed |= changed;
        self.still = still;
    }

    /// The device was made again: its pass goes, and the painter's
    /// textures went with the old one.
    pub fn device_changed(&mut self) {
        self.pipeline = None;
        self.filled.clear();
        self.changed = false;
    }

    /// The source it is drawn under, while it has one.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref().filter(|_| self.still.is_some())
    }

    /// Takes the painter's requests for the still's source, drawing each
    /// (and again those drawn before, if it changed): what is left is the
    /// platform's to read.
    pub fn serve(
        &mut self,
        painter: &mut Painter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        requests: Vec<(TextureId, TextureKey)>,
    ) -> Vec<(TextureId, TextureKey)> {
        if !self.stale.is_empty() {
            let stale = std::mem::take(&mut self.stale);
            painter.forget_textures(|s| stale.iter().any(|t| t == s));
        }
        if std::mem::take(&mut self.changed) {
            for (id, side) in self.filled.clone() {
                self.fill(painter, device, queue, id, side);
            }
        }
        let Some(source) = self.source.clone() else {
            return requests;
        };
        let limit = painter.max_texture_side();
        let mut rest = Vec::new();
        for (id, key) in requests {
            if *key.source != *source {
                rest.push((id, key));
                continue;
            }
            if self.still.is_none() {
                painter.fail_texture(id);
                continue;
            }
            let side = match key.max_side {
                0 => limit,
                s => s.min(limit),
            };
            self.fill(painter, device, queue, id, side);
            self.filled.retain(|(i, _)| *i != id);
            self.filled.push((id, side));
        }
        rest
    }

    /// Draws the still into a new texture of the painter's, `id`, its
    /// longer side `side`.
    fn fill(
        &mut self,
        painter: &mut Painter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: TextureId,
        side: u32,
    ) {
        let Some(still) = &self.still else {
            return;
        };
        let size = still_size(still.aspect, side);
        let pipeline = self
            .pipeline
            .get_or_insert_with(|| GenerativePipeline::new(device, queue));
        pipeline.set_picture(
            device,
            still
                .picture
                .as_ref()
                .map(|(v, size, linear)| (v, *size, *linear)),
        );
        let frame = Frame {
            look: still.want.look,
            palette: still.want.palette,
            signals: &still.signals,
            light: still.light,
            picture: pipeline.has_picture(),
            picture_brightness: 1.0,
        };
        // Its first level, drawn; the painter makes the rest.
        painter.insert_texture(device, id, size, |texture| {
            pipeline.draw_still(device, queue, texture, &frame);
        });
    }
}

/// Whether `backdrop` draws generated background `id`.
pub fn shown(backdrop: &ddi_render::Backdrop, id: usize) -> bool {
    backdrop
        .first
        .iter()
        .chain(&backdrop.second)
        .flatten()
        .any(|d| d.id == id && d.color[3] > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(format: &str) -> ddi_chart::Song {
        let mut song = ddi_chart::formats::sm::parse_simfile(
            "#TITLE:t;#BPMS:0=120;\n#NOTES:dance-single::Easy:1::\n1000\n;",
            "sm",
        )
        .unwrap();
        if format == "danoni" {
            song.source.format = ddi_chart::SourceFormat::Danoni;
        }
        song
    }

    #[test]
    fn every_song_but_a_work_gets_the_chosen_look() {
        let mut settings = Settings::default();
        let want = wanted(&settings, &song("sm"), false).unwrap();
        assert_eq!(
            (want.look, want.palette, want.over),
            (Look::Field, Palette::Ink, false)
        );
        assert_eq!(wanted(&settings, &song("danoni"), false), None);
        // Never in calibration, nor with the backgrounds off.
        assert_eq!(wanted(&settings, &song("sm"), true), None);
        let dark = Settings {
            bg_brightness: 0.0,
            ..settings.clone()
        };
        assert_eq!(wanted(&dark, &song("sm"), false), None);
        settings.generated_background = ddi_ui::state::settings::GeneratedBackground::Off;
        assert_eq!(wanted(&settings, &song("sm"), false), None);
    }

    #[test]
    fn the_prompt_shows_the_songs_picture_dimmed_else_the_still_as_drawn() {
        let pic = |s: &str| Some((s.to_string(), 1.5));
        let at = |p: Option<PromptPicture>| p.map(|p| (p.source, p.brightness));
        let shown =
            |song, still, brightness| at(prompt_picture(song, still, brightness, None, None));
        assert_eq!(
            shown(pic("song"), pic("still"), 0.4),
            Some(("song".into(), 0.4))
        );
        assert_eq!(shown(None, pic("still"), 0.4), Some(("still".into(), 1.0)));
        assert_eq!(shown(None, None, 0.4), None);
        assert_eq!(shown(pic("song"), pic("still"), 0.0), None);
    }

    #[test]
    fn the_music_is_analyzed_only_where_the_look_may_show() {
        let want = Want {
            look: Look::Rain,
            palette: Palette::Ink,
            over: false,
        };
        assert!(needs_music(Some(want), false));
        assert!(!needs_music(Some(want), true));
        assert!(needs_music(Some(Want { over: true, ..want }), true));
        assert!(!needs_music(None, false));
    }

    #[test]
    fn the_kaleidoscope_folds_the_cover_where_the_song_has_no_picture() {
        let want = |look, over| Want {
            look,
            palette: Palette::Ink,
            over,
        };
        let k = want(Look::Kaleidoscope, false);
        assert_eq!(folded(k, None, Some(7)), Some(7));
        assert_eq!(folded(k, None, None), None);
        // Over a picture, the picture.
        let over = want(Look::Kaleidoscope, true);
        assert_eq!(folded(over, Some(3), Some(7)), Some(3));
        assert_eq!(folded(over, None, Some(7)), Some(7));
        // The others draw over the song's picture only.
        assert_eq!(folded(want(Look::Bars, false), Some(3), Some(7)), None);
        assert_eq!(folded(want(Look::Bars, true), Some(3), Some(7)), Some(3));
        assert_eq!(folded(want(Look::Field, true), None, Some(7)), None);
        // The cover is read for the kaleidoscope standing in alone.
        assert!(needs_cover(Some(k), false));
        assert!(!needs_cover(Some(k), true));
        assert!(!needs_cover(Some(want(Look::Rain, false)), false));
        assert!(!needs_cover(None, false));
    }

    #[test]
    fn the_cover_is_the_jacket_else_the_banner() {
        let mut entry = ddi_library::manifest::summarize(Default::default(), &song("sm"));
        entry.banner = None;
        entry.jacket = None;
        assert_eq!(cover(&entry), None);
        entry.banner = Some("banner.png".into());
        assert_eq!(cover(&entry).map(|p| p.as_str()), Some("banner.png"));
        entry.jacket = Some("jacket.png".into());
        assert_eq!(cover(&entry).map(|p| p.as_str()), Some("jacket.png"));
    }

    #[test]
    fn a_still_keeps_the_frames_shape_at_the_side_asked() {
        assert_eq!(still_size(16.0 / 9.0, 1280), [1280, 720]);
        assert_eq!(still_size(4.0 / 3.0, 64), [64, 48]);
        assert_eq!(still_size(0.5, 100), [50, 100]);
        assert_eq!(still_size(f32::NAN, 10), [10, 10]);
        assert_eq!(still_size(1000.0, 10), [10, 1]);
    }

    #[test]
    fn full_at_the_default_brightness_dimmer_under_it() {
        assert_eq!(light(0.4), 1.0);
        assert_eq!(light(1.0), 1.0);
        assert_eq!(light(0.2), 0.5);
        assert_eq!(light(0.0), 0.0);
    }

    #[test]
    fn the_texture_is_the_frame_within_the_devices_largest() {
        assert_eq!(texture_size([1280.0, 720.0], 8192), [1280, 720]);
        assert_eq!(texture_size([3840.0, 2160.0], 8192), [3840, 2160]);
        assert_eq!(texture_size([3840.0, 2160.0], 2048), [2048, 1152]);
        assert_eq!(texture_size([1440.0, 3440.0], 2048), [857, 2048]);
        assert_eq!(texture_size([0.0, 0.0], 2048), [1, 1]);
    }
}
