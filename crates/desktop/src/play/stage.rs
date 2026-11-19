//! A Dancing☆Onigiri work's back and mask layers on desktop
//! (`docs/plans/overlays.md`, `docs/plans/desktop.md` 7.5b), as the web's
//! `stage.rs` shows them: the contents a song's layers show (its images,
//! read where the library found them; its text, drawn here as the web's
//! canvas draws it) read on a thread once per song, written to their
//! textures by the shared view (`ddi_shell::stage`).

use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock};

use ddi_engine::sprites::SpriteSource;
use ddi_session::stage::{MAX_LAYER_SIDE, TEXT_SCALE};

use super::view::Rgba;

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

    /// What the layers' timeline calls it.
    pub fn source(&self) -> SpriteSource {
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
    ddi_session::stage::layer_sources(song, chart)
        .iter()
        .filter_map(|s| match s {
            SpriteSource::Image(name) => layers
                .iter()
                .find(|(n, _)| **n == **name)
                .map(|(_, source)| (Content::of(s), Some(source.clone()))),
            SpriteSource::Text { .. } => Some((Content::of(s), None)),
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

/// The fonts text is drawn with: the system's (`system`), else none;
/// where that gives no face at all (a Linux with no fonts installed), the
/// bundled ones, Iosevka Term standing for sans-serif
/// (`ddi_render::cosmic::back_with`): cosmic-text panics with no face.
fn font_system(system: bool) -> glyphon::cosmic_text::FontSystem {
    let mut fonts = if system {
        glyphon::cosmic_text::FontSystem::new()
    } else {
        glyphon::cosmic_text::FontSystem::new_with_locale_and_db(
            "en-US".into(),
            glyphon::cosmic_text::fontdb::Database::new(),
        )
    };
    if ddi_render::cosmic::back_with(&mut fonts, &crate::gpu::faces(), "Iosevka Term") {
        eprintln!("No system fonts: a work's text is drawn in the bundled faces");
    }
    fonts
}

/// The fonts text is drawn with ([`font_system`]), made once.
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
        Mutex::new((font_system(true), glyphon::cosmic_text::SwashCache::new()))
    })
}

/// A text content drawn as the web's canvas draws it (`text_bitmap`):
/// its font size (16 px for the page's own, 400 at most) at
/// [`TEXT_SCALE`] times, in the page's sans-serif face, line by line (64
/// at most) 1.2 lines apart from the top, in its color (`#cccccc` for
/// none or one not read); its pixels and its size in the stage's px.
pub fn text_image(text: &str, size: u32, color: &str) -> Option<(Rgba, (f32, f32))> {
    let mut guard = fonts().lock().ok()?;
    let (fonts, swash) = &mut *guard;
    Some(text_image_in(fonts, swash, text, size, color))
}

/// [`text_image`] in `fonts`: a glyph in no face (its first face's
/// .notdef) draws nothing, its room kept.
fn text_image_in(
    fonts: &mut glyphon::cosmic_text::FontSystem,
    swash: &mut glyphon::cosmic_text::SwashCache,
    text: &str,
    size: u32,
    color: &str,
) -> (Rgba, (f32, f32)) {
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
    // No face at all: nothing drawn (cosmic-text would panic).
    if fonts.db().is_empty() {
        let h = ((line * lines.len() as f32).ceil() as u32).clamp(1, 2048);
        return (
            Rgba {
                size: [1, h],
                data: vec![0; (h * 4) as usize],
            },
            (1.0 / TEXT_SCALE, h as f32 / TEXT_SCALE),
        );
    }
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
    let base = Color::rgba(r, g, b, a);
    // As `Buffer::draw` draws, but a glyph in no face (its first face's
    // .notdef, glyph 0) draws nothing.
    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter().filter(|g| g.glyph_id != 0) {
            let p = glyph.physical((0.0, run.line_y), 1.0);
            let color = glyph.color_opt.unwrap_or(base);
            swash.with_pixels(fonts, p.cache_key, color, |x, y, c| {
                let alpha = c.a();
                let (x, y) = (p.x + x, p.y + y);
                if alpha == 0 || x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                    return;
                }
                let i = ((y as u32 * w + x as u32) * 4) as usize;
                // Glyphs do not overlap: the coverage is the pixel's.
                let px = &mut data[i..i + 4];
                if alpha >= px[3] {
                    px.copy_from_slice(&[c.r(), c.g(), c.b(), alpha]);
                }
            });
        }
    }
    (
        Rgba { size: [w, h], data },
        (w as f32 / TEXT_SCALE, h as f32 / TEXT_SCALE),
    )
}

/// A CSS color as a canvas's `fillStyle` reads it (every named color,
/// `transparent`, hex, `rgb()`, `hsl()`, `hwb()` and the rest,
/// `csscolorparser`); `None` for one it does not read (the canvas keeps
/// its color then).
pub fn css_color(s: &str) -> Option<[u8; 4]> {
    csscolorparser::parse(s.trim()).ok().map(|c| c.to_rgba8())
}

/// A content read is copied to its texture as RGBA rows.
impl ddi_shell::stage::StageContent for Rgba {
    fn size(&self) -> [u32; 2] {
        self.size
    }

    fn write(&self, queue: &wgpu::Queue, texture: &wgpu::Texture, _: [u32; 2]) {
        super::view::write_rgba(queue, texture, self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_as_a_canvas_reads_them() {
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

    /// With no system font at all (a Linux with none installed) a work's
    /// text is drawn in the bundled faces, never a panic ("no default font
    /// found"): Latin in Iosevka Term, Japanese in its bundled face; a
    /// character in no face draws nothing, its room kept.
    #[test]
    fn with_no_system_font_text_is_drawn_in_the_bundled_faces() {
        let mut fonts = font_system(false);
        assert!(!fonts.db().is_empty(), "the bundled faces");
        let mut swash = glyphon::cosmic_text::SwashCache::new();
        let mut draw = |text: &str| text_image_in(&mut fonts, &mut swash, text, 24, "#fff");
        let inked = |image: &Rgba| image.data.chunks(4).filter(|p| p[3] > 0).count();
        let (latin, _) = draw("Hello");
        assert!(inked(&latin) > 100, "Latin drawn");
        let (japanese, _) = draw("日本語");
        assert!(inked(&japanese) > 100, "Japanese drawn");
        // A hieroglyph no bundled face has: nothing drawn, its room kept.
        let (none, _) = draw("\u{13000}");
        assert_eq!(inked(&none), 0, "a glyph in no face draws nothing");
        let (with, _) = draw("Hello\u{13000}");
        assert_eq!(inked(&with), inked(&latin));
        assert!(with.size[0] >= latin.size[0]);
        // And with no face at all (no bundled one either): nothing, no
        // panic.
        let mut empty = glyphon::cosmic_text::FontSystem::new_with_locale_and_db(
            "en-US".into(),
            glyphon::cosmic_text::fontdb::Database::new(),
        );
        let (nothing, _) = text_image_in(&mut empty, &mut swash, "Hello", 24, "#fff");
        assert_eq!(inked(&nothing), 0);
    }

    #[test]
    fn text_is_drawn_at_three_times_its_size() {
        let (image, size) = text_image("Hello\nworld", 24, "#fff").unwrap();
        assert_eq!(image.size[1], (24.0 * 3.0 * 1.2 * 2.0f32).ceil() as u32);
        assert!((size.1 - 24.0 * 1.2 * 2.0).abs() < 0.5, "{size:?}");
        assert!(image.data.chunks(4).any(|p| p[3] > 200 && p[0] == 255));
    }
}
