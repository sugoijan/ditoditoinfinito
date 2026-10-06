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
    cache: HashMap<TextKey, CachedBuffer>,
    frame: u64,
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
        let mut font_system = FontSystem::new_with_fonts(
            fonts
                .iter()
                .map(|f| glyphon::fontdb::Source::Binary(std::sync::Arc::new(*f))),
        );
        // Make sure fallback works even with no embedded font.
        font_system.db_mut().set_sans_serif_family("sans-serif");
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let viewport = Viewport::new(device, &cache);
        let renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        TextPipeline {
            font_system,
            swash: SwashCache::new(),
            atlas,
            viewport,
            renderer,
            cache: HashMap::new(),
            frame: 0,
        }
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
        for item in items {
            let entry = self
                .cache
                .entry(TextKey::of(item))
                .or_insert_with(|| shape(font_system, item));
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

    /// Call after a frame when the atlas may hold stale glyphs.
    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}

fn shape(font_system: &mut FontSystem, item: &TextItem) -> CachedBuffer {
    let mut buffer = Buffer::new(font_system, Metrics::new(item.size, item.size * 1.2));
    buffer.set_size(None, None);
    let attrs = Attrs::new().family(Family::SansSerif).weight(if item.bold {
        glyphon::Weight::BOLD
    } else {
        glyphon::Weight::NORMAL
    });
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
