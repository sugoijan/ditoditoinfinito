//! A Dancing☆Onigiri work's back and mask layers on desktop
//! (`docs/plans/overlays.md`, `docs/plans/desktop.md` 7.5b), as the web's
//! `stage.rs` shows them: the contents a song's layers show (its images,
//! read where the library found them; its text, drawn here as the web's
//! canvas draws it) read on a thread once per song, their textures
//! (registered as a noteskin's, so again after a skin replaces them), and
//! the play's timeline (`ddi_session::stage`).

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock};

use ddi_engine::sprites::{SpriteSource, SpriteTrack};
use ddi_render::Renderer;
use ddi_render::stage::{self, StageTexture};
use ddi_render::textured::TexQuad;
use ddi_session::stage::StageLayers;

use super::view::Rgba;

/// The longest side an image is kept at (the web's `MAX_LAYER_SIDE`).
const MAX_LAYER_SIDE: u32 = 2048;
/// Texts drawn at most (the web's `MAX_LAYER_TEXTS`).
const MAX_LAYER_TEXTS: usize = 64;
/// How much finer than its size text is drawn (the web's `TEXT_SCALE`).
const TEXT_SCALE: f32 = 3.0;

/// A content as it crosses threads ([`SpriteSource`] holds `Rc`s).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    Image(String),
    Text {
        text: String,
        size: u32,
        color: String,
    },
}

impl Content {
    fn of(s: &SpriteSource) -> Content {
        match s {
            SpriteSource::Image(name) => Content::Image(name.to_string()),
            SpriteSource::Text { text, size, color } => Content::Text {
                text: text.to_string(),
                size: *size,
                color: color.to_string(),
            },
        }
    }

    fn source(&self) -> SpriteSource {
        match self {
            Content::Image(name) => SpriteSource::Image(Rc::from(name.as_str())),
            Content::Text { text, size, color } => SpriteSource::Text {
                text: Rc::from(text.as_str()),
                size: *size,
                color: Rc::from(color.as_str()),
            },
        }
    }
}

/// A content read: its pixels, and its size in the stage's px.
pub type Read = (Content, Rgba, (f32, f32));

/// What chart `chart` of `song` shows on its layers (under both scroll
/// directions): its images among `layers` (`(name, texture source)`, those
/// the library found) and its texts, within the budgets.
pub fn contents(
    song: &ddi_chart::Song,
    chart: usize,
    layers: &[(String, String)],
) -> Vec<(Content, Option<String>)> {
    let mut sources = SpriteTrack::new(song, chart, false).sources();
    for s in SpriteTrack::new(song, chart, true).sources() {
        if !sources.contains(&s) {
            sources.push(s);
        }
    }
    let mut texts = 0;
    sources
        .iter()
        .filter_map(|s| match s {
            SpriteSource::Image(name) => layers
                .iter()
                .find(|(n, _)| **n == **name)
                .map(|(_, source)| (Content::of(s), Some(source.clone()))),
            SpriteSource::Text { .. } => {
                texts += 1;
                (texts <= MAX_LAYER_TEXTS).then(|| (Content::of(s), None))
            }
        })
        .collect()
}

/// Reads `contents` on a thread of its own: each comes through the
/// receiver as it is read (`wake` wakes the loop).
pub fn read(
    contents: Vec<(Content, Option<String>)>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) -> Receiver<Read> {
    let (tx, rx) = mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("ddi-stage".into())
        .spawn(move || {
            for (content, source) in contents {
                let read = match (&content, source) {
                    (Content::Image(_), Some(source)) => {
                        crate::images::load_sized(&source, MAX_LAYER_SIDE)
                            .map(|(size, data, (w, h))| (Rgba { size, data }, (w as f32, h as f32)))
                    }
                    (Content::Text { text, size, color }, _) => text_image(text, *size, color),
                    _ => None,
                };
                let Some((image, size)) = read else {
                    eprintln!("stage: {content:?} could not be read");
                    continue;
                };
                if tx.send((content, image, size)).is_err() {
                    return;
                }
                if let Some(w) = &wake {
                    w();
                }
            }
        });
    rx
}

/// The fonts text is drawn with: the system's, made once.
fn fonts() -> &'static Mutex<(
    glyphon::cosmic_text::FontSystem,
    glyphon::cosmic_text::SwashCache,
)> {
    static FONTS: OnceLock<
        Mutex<(
            glyphon::cosmic_text::FontSystem,
            glyphon::cosmic_text::SwashCache,
        )>,
    > = OnceLock::new();
    FONTS.get_or_init(|| {
        Mutex::new((
            glyphon::cosmic_text::FontSystem::new(),
            glyphon::cosmic_text::SwashCache::new(),
        ))
    })
}

/// A text content drawn as the web's canvas draws it (`text_bitmap`):
/// its font size (16 px for the page's own, 400 at most) at
/// [`TEXT_SCALE`] times, in the page's sans-serif face, line by line (64
/// at most) 1.2 lines apart from the top, in its colour (`#cccccc` for
/// none or one not read); its pixels and its size in the stage's px.
pub fn text_image(text: &str, size: u32, color: &str) -> Option<(Rgba, (f32, f32))> {
    use glyphon::cosmic_text::{Attrs, Buffer, Color, Family, Metrics, Shaping};
    let size = if size == 0 {
        16.0
    } else {
        size.min(400) as f32
    };
    let px = size * TEXT_SCALE;
    let line = px * 1.2;
    let lines: Vec<&str> = text.split('\n').take(64).collect();
    let [r, g, b, a] = css_color(color).unwrap_or([0xcc, 0xcc, 0xcc, 0xff]);
    let mut guard = fonts().lock().ok()?;
    let (fonts, swash) = &mut *guard;
    let mut buffer = Buffer::new(fonts, Metrics::new(px, line));
    buffer.set_size(None, None);
    let attrs = Attrs::new().family(Family::SansSerif);
    buffer.set_text(&lines.join("\n"), &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    let width = buffer
        .layout_runs()
        .map(|r| r.line_w)
        .fold(0.0f32, f32::max);
    let (w, h) = (
        (width.ceil() as u32).clamp(1, 2048),
        ((line * lines.len() as f32).ceil() as u32).clamp(1, 2048),
    );
    let mut data = vec![0u8; (w * h * 4) as usize];
    buffer.draw(fonts, swash, Color::rgba(r, g, b, a), |x, y, gw, gh, c| {
        let alpha = c.a();
        if alpha == 0 {
            return;
        }
        for yy in y.max(0)..(y + gh as i32).min(h as i32) {
            for xx in x.max(0)..(x + gw as i32).min(w as i32) {
                let i = ((yy as u32 * w + xx as u32) * 4) as usize;
                // Glyphs do not overlap: the coverage is the pixel's.
                let px = &mut data[i..i + 4];
                if alpha >= px[3] {
                    px.copy_from_slice(&[c.r(), c.g(), c.b(), alpha]);
                }
            }
        }
    });
    Some((
        Rgba { size: [w, h], data },
        (w as f32 / TEXT_SCALE, h as f32 / TEXT_SCALE),
    ))
}

/// A CSS colour as a canvas's `fillStyle` reads it (every named colour,
/// `transparent`, hex, `rgb()`, `hsl()`, `hwb()` and the rest,
/// `csscolorparser`); `None` for one it does not read (the canvas keeps
/// its colour then).
pub fn css_color(s: &str) -> Option<[u8; 4]> {
    csscolorparser::parse(s.trim()).ok().map(|c| c.to_rgba8())
}

/// The layers in a play: the contents read, their textures, the play's
/// timeline.
#[derive(Default)]
pub struct StageView {
    layers: StageLayers,
    /// Contents read and their size in the stage's px (a retry keeps
    /// them).
    contents: HashMap<SpriteSource, (Rgba, (f32, f32))>,
    /// Contents on the GPU since the textures were last cleared.
    textures: HashMap<SpriteSource, StageTexture>,
}

impl StageView {
    /// The layers of the play about to start (`None`: none shown).
    pub fn set_track(&mut self, track: Option<SpriteTrack>) {
        self.layers.set_track(track);
    }

    /// Whether a play's layers are shown.
    pub fn shown(&self) -> bool {
        self.layers.shown()
    }

    /// A content read.
    pub fn add(&mut self, read: Read) {
        let (content, image, size) = read;
        let source = content.source();
        self.textures.remove(&source);
        self.contents.insert(source, (image, size));
    }

    /// Contents read so far.
    pub fn contents(&self) -> usize {
        self.contents.len()
    }

    /// The renderer's skin textures were cleared, and these with them.
    pub fn forget_textures(&mut self) {
        self.textures.clear();
    }

    /// Puts the contents not on the GPU yet there.
    pub fn upload(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        for (source, (image, size)) in &self.contents {
            if self.textures.contains_key(source) {
                continue;
            }
            let [w, h] = image.size;
            let texture = renderer.create_noteskin_texture(device, w.max(1), h.max(1), false);
            super::view::write_rgba(queue, &texture, image);
            let id = renderer.add_noteskin_texture(device, &texture);
            self.textures
                .insert(source.clone(), StageTexture { id, size: *size });
        }
    }

    /// The back and mask quads at song second `t`, on a target `height`
    /// px tall with the field centred at `center_x`.
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
    use super::*;

    #[test]
    fn colours_as_a_canvas_reads_them() {
        assert_eq!(css_color("#f00"), Some([255, 0, 0, 255]));
        assert_eq!(css_color("#00ff0080"), Some([0, 255, 0, 128]));
        assert_eq!(css_color("rgba(10, 20, 30, 0.5)"), Some([10, 20, 30, 128]));
        assert_eq!(css_color("White"), Some([255, 255, 255, 255]));
        assert_eq!(css_color("rebeccapurple"), Some([102, 51, 153, 255]));
        assert_eq!(css_color("hsl(120, 100%, 50%)"), Some([0, 255, 0, 255]));
        assert_eq!(css_color("hsla(0 100% 50% / 0.5)"), Some([255, 0, 0, 128]));
        assert_eq!(css_color("transparent"), Some([0, 0, 0, 0]));
        assert_eq!(css_color("nonsense"), None);
    }

    #[test]
    fn text_is_drawn_at_three_times_its_size() {
        let (image, size) = text_image("Hello\nworld", 24, "#fff").unwrap();
        assert_eq!(image.size[1], (24.0 * 3.0 * 1.2 * 2.0f32).ceil() as u32);
        assert!((size.1 - 24.0 * 1.2 * 2.0).abs() < 0.5, "{size:?}");
        assert!(image.data.chunks(4).any(|p| p[3] > 200 && p[0] == 255));
    }
}
