//! Text rendering through glyphon (cosmic-text atlas on wgpu).
//!
//! Shaped [`Buffer`]s are cached per `(text, size, weight)`: the HUD shows a
//! handful of strings that change rarely (score, combo, judgement), and
//! shaping them again every frame is the expensive part of text rendering.

use std::collections::HashMap;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

/// Horizontal anchor of a text item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// One piece of text to draw this frame, in pixels.
#[derive(Clone, Debug)]
pub struct TextItem {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: [f32; 4],
    pub align: Align,
    pub bold: bool,
}

/// Frames a cached buffer may go unused before it is dropped.
const CACHE_TTL_FRAMES: u64 = 120;

#[derive(Clone, PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    /// `f32::to_bits` of the font size.
    size_bits: u32,
    bold: bool,
}

impl TextKey {
    fn of(item: &TextItem) -> TextKey {
        TextKey {
            text: item.text.clone(),
            size_bits: item.size.to_bits(),
            bold: item.bold,
        }
    }
}

struct CachedBuffer {
    buffer: Buffer,
    /// Widest layout run, for alignment.
    line_w: f32,
    last_used: u64,
}

pub struct TextPipeline {
    font_system: FontSystem,
    swash: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    renderer: TextRenderer,
    /// Glyphon's pipelines, for another target format.
    glyphon_cache: Cache,
    cache: HashMap<TextKey, CachedBuffer>,
    frame: u64,
    /// Runs shaped since the start (each a cache miss: a new string or
    /// size).
    shaped: u64,
}

impl TextPipeline {
    /// `fonts` are TTF/OTF byte blobs embedded by the shell; the first family
    /// found is used for everything.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        fonts: &[&'static [u8]],
    ) -> TextPipeline {
        // Only the embedded fonts, as on the web (where there are no
        // others): the HUD looks the same on every platform. A system's
        // fonts would win the match on desktop (cosmic-text prefers its
        // own system face over a named embedded one there).
        let mut db = glyphon::fontdb::Database::new();
        for f in fonts {
            db.load_font_source(glyphon::fontdb::Source::Binary(std::sync::Arc::new(*f)));
        }
        // The first family for everything.
        let family = db
            .faces()
            .next()
            .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
            .unwrap_or_else(|| "sans-serif".to_string());
        db.set_sans_serif_family(family);
        let font_system = FontSystem::new_with_locale_and_db("en-US".into(), db);
        let cache = Cache::new(device);
        let mut atlas =
            TextAtlas::with_color_mode(device, queue, &cache, format, color_mode(format));
        let viewport = Viewport::new(device, &cache);
        let renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        TextPipeline {
            font_system,
            swash: SwashCache::new(),
            atlas,
            viewport,
            renderer,
            glyphon_cache: cache,
            cache: HashMap::new(),
            frame: 0,
            shaped: 0,
        }
    }

    /// Draws to `format` from now on: a new glyph atlas (glyphon's is made
    /// for one format), filled again as text is drawn.
    pub fn retarget(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        self.atlas = TextAtlas::with_color_mode(
            device,
            queue,
            &self.glyphon_cache,
            format,
            color_mode(format),
        );
        self.renderer = TextRenderer::new(
            &mut self.atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );
    }

    /// Shape and upload `items` for a target of `width × height` pixels.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        items: &[TextItem],
    ) -> Result<(), glyphon::PrepareError> {
        self.viewport.update(queue, Resolution { width, height });
        self.frame += 1;
        let frame = self.frame;
        let font_system = &mut self.font_system;
        let shaped = &mut self.shaped;
        for item in items {
            let entry = self.cache.entry(TextKey::of(item)).or_insert_with(|| {
                *shaped += 1;
                shape(font_system, item)
            });
            entry.last_used = frame;
        }
        self.cache
            .retain(|_, e| frame - e.last_used < CACHE_TTL_FRAMES);
        let mut areas = Vec::with_capacity(items.len());
        for item in items {
            let Some(cached) = self.cache.get(&TextKey::of(item)) else {
                continue;
            };
            let left = match item.align {
                Align::Left => item.x,
                Align::Center => item.x - cached.line_w / 2.0,
                Align::Right => item.x - cached.line_w,
            };
            let c = item.color;
            areas.push(TextArea {
                buffer: &cached.buffer,
                left,
                top: item.y,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                },
                default_color: Color::rgba(
                    (c[0] * 255.0) as u8,
                    (c[1] * 255.0) as u8,
                    (c[2] * 255.0) as u8,
                    (c[3] * 255.0) as u8,
                ),
                custom_glyphs: &[],
            });
        }
        self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash,
        )
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let _ = self.renderer.render(&self.atlas, &self.viewport, pass);
    }

    /// Runs shaped (and their glyphs rasterised) since the start.
    pub fn shaped(&self) -> u64 {
        self.shaped
    }

    /// Call after a frame when the atlas may hold stale glyphs.
    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}

/// How glyphon takes a run's colour (sRGB values, as written) for a
/// target: turned into linear light where the target holds linear light
/// (an sRGB view, which encodes it again, or a float texture encoded at
/// the end); as they are where it holds encoded values (a plain view).
/// So a colour draws as written on every target.
pub fn color_mode(format: wgpu::TextureFormat) -> glyphon::ColorMode {
    use wgpu::TextureFormat as F;
    let linear = format.is_srgb()
        || matches!(
            format,
            F::Rgba16Float | F::Rgba32Float | F::Rg11b10Ufloat | F::Rgb9e5Ufloat
        );
    if linear {
        glyphon::ColorMode::Accurate
    } else {
        glyphon::ColorMode::Web
    }
}

fn shape(font_system: &mut FontSystem, item: &TextItem) -> CachedBuffer {
    let mut buffer = Buffer::new(font_system, Metrics::new(item.size, item.size * 1.2));
    buffer.set_size(None, None);
    // Unhinted: the HUD's face is hinted, and hinting draws a run at its
    // size rounded to a whole pixel while it is laid out at the exact one
    // (and a pop's cached sizes, `scene::popped`, a whole pixel off the
    // exact ones). The HUD's sizes are fractions of an arrow, and large.
    let attrs = Attrs::new()
        .family(Family::SansSerif)
        .weight(if item.bold {
            glyphon::Weight::BOLD
        } else {
            glyphon::Weight::NORMAL
        })
        .cache_key_flags(glyphon::cosmic_text::CacheKeyFlags::DISABLE_HINTING);
    buffer.set_text(&item.text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    let line_w = buffer
        .layout_runs()
        .map(|r| r.line_w)
        .fold(0.0_f32, f32::max);
    CachedBuffer {
        buffer,
        line_w,
        last_used: 0,
    }
}
