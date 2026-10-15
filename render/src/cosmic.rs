//! Text runs drawn by cosmic-text (`docs/plans/ui-core.md`, P4): the
//! [`RunRasterizer`] of desktop, embedded targets and native tests. The
//! web draws runs with the browser instead (`app/src/web/text.rs`); this
//! one stays buildable there, as the way back should the browser's text
//! ever be dropped.
//!
//! Fonts are the bundled ones given, and the system's when asked for
//! (cosmic-text falls back per script for glyphs a font lacks). A style's
//! family is a CSS font stack; its first name is looked up, CSS's generic
//! names standing for cosmic-text's.

use ddi_actor::text::{RunMetrics, TextStyle};
use glyphon::cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent, Weight,
};

use crate::atlas::PageRect;
use crate::text_atlas::{PAGE, RunRasterizer};

pub struct CosmicRasterizer {
    fonts: FontSystem,
    swash: SwashCache,
    /// The staging page: coverage, one byte a texel.
    staging: Vec<u8>,
}

/// One glyph's coverage and where its top left goes, from the pen.
struct Glyph {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    /// Coverage (a mask), or RGBA whose alpha is taken (a colour glyph).
    data: Vec<u8>,
    rgba: bool,
}

impl CosmicRasterizer {
    /// With the `bundled` faces, each under the family name given (as the
    /// web registers a `FontFace` under its name: a face's own name may
    /// carry its weight, "Iosevka Term Semibold"), and the system's with
    /// `system`.
    pub fn with_families(bundled: &[(&str, &[u8])], system: bool) -> CosmicRasterizer {
        let mut r = CosmicRasterizer::new(&[], system);
        let db = r.fonts.db_mut();
        for (family, bytes) in bundled {
            let source =
                glyphon::cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(bytes.to_vec()));
            for id in db.load_font_source(source) {
                let Some(mut info) = db.face(id).cloned() else {
                    continue;
                };
                db.remove_face(id);
                info.families = vec![(
                    family.to_string(),
                    glyphon::cosmic_text::fontdb::Language::English_UnitedStates,
                )];
                db.push_face_info(info);
            }
        }
        r
    }

    /// With the `bundled` fonts (TTF or OTF bytes), and the system's with
    /// `system`.
    pub fn new(bundled: &[&[u8]], system: bool) -> CosmicRasterizer {
        let mut fonts = if system {
            FontSystem::new()
        } else {
            FontSystem::new_with_locale_and_db(
                "en-US".into(),
                glyphon::cosmic_text::fontdb::Database::new(),
            )
        };
        for bytes in bundled {
            fonts.db_mut().load_font_data(bytes.to_vec());
        }
        CosmicRasterizer {
            fonts,
            swash: SwashCache::new(),
            staging: vec![0; (PAGE * PAGE) as usize],
        }
    }

    /// The staging page's coverage (tests).
    pub fn staging(&self) -> &[u8] {
        &self.staging
    }

    /// The run laid out at `scale`: its advance, the baseline from the
    /// line box's top, and its glyphs placed from the pen.
    fn layout(&mut self, text: &str, style: &TextStyle, scale: f32) -> (f32, f32, Vec<Glyph>) {
        let size = (style.size() * scale).max(1.0);
        let metrics = Metrics::new(size, style.line_height() * scale);
        let mut buffer = Buffer::new(&mut self.fonts, metrics);
        let family = first_family(&style.family);
        let attrs = Attrs::new()
            .family(match family.as_str() {
                "sans-serif" | "system-ui" | "ui-sans-serif" => Family::SansSerif,
                "serif" | "ui-serif" => Family::Serif,
                "monospace" | "ui-monospace" => Family::Monospace,
                "cursive" => Family::Cursive,
                "fantasy" => Family::Fantasy,
                name => Family::Name(name),
            })
            .weight(Weight(style.weight))
            .style(if style.italic {
                Style::Italic
            } else {
                Style::Normal
            });
        let attrs = if style.tracking != 0 {
            attrs.letter_spacing(f32::from(style.tracking) / 1000.0)
        } else {
            attrs
        };
        buffer.set_size(None, None);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);
        let mut advance = 0.0f32;
        let mut baseline = 0.0f32;
        let mut glyphs = Vec::new();
        // One line: the first layout run.
        if let Some(run) = buffer.layout_runs().next() {
            advance = run.line_w;
            baseline = run.line_y - run.line_top;
            for g in run.glyphs {
                let physical = g.physical((0.0, 0.0), 1.0);
                let Some(image) = self
                    .swash
                    .get_image(&mut self.fonts, physical.cache_key)
                    .as_ref()
                else {
                    continue;
                };
                glyphs.push(Glyph {
                    left: physical.x + image.placement.left,
                    top: physical.y - image.placement.top,
                    width: image.placement.width,
                    height: image.placement.height,
                    rgba: matches!(image.content, SwashContent::Color),
                    data: image.data.clone(),
                });
            }
        }
        (advance, baseline, glyphs)
    }
}

/// The first family of a CSS font stack, unquoted.
fn first_family(stack: &str) -> String {
    stack
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string()
}

impl RunRasterizer for CosmicRasterizer {
    fn measure(&mut self, text: &str, style: &TextStyle, scale: f32) -> RunMetrics {
        let (advance, baseline, glyphs) = self.layout(text, style, scale);
        let mut ink = [0.0f32; 4];
        let mut any = false;
        for g in glyphs.iter().filter(|g| g.width > 0 && g.height > 0) {
            let r = [
                g.left as f32,
                g.top as f32,
                (g.left + g.width as i32) as f32,
                (g.top + g.height as i32) as f32,
            ];
            if any {
                ink = [
                    ink[0].min(r[0]),
                    ink[1].min(r[1]),
                    ink[2].max(r[2]),
                    ink[3].max(r[3]),
                ];
            } else {
                ink = r;
                any = true;
            }
        }
        RunMetrics {
            advance,
            ink,
            baseline,
        }
    }

    fn draw(&mut self, text: &str, style: &TextStyle, scale: f32, pen: [f32; 2], cell: PageRect) {
        let (_, _, glyphs) = self.layout(text, style, scale);
        let (px, py) = (pen[0].round() as i32, pen[1].round() as i32);
        let side = PAGE as i32;
        let (xs, ys) = (
            cell.x as i32..(cell.right() as i32).min(side),
            cell.y as i32..(cell.bottom() as i32).min(side),
        );
        for g in glyphs {
            for row in 0..g.height as i32 {
                let y = py + g.top + row;
                if !ys.contains(&y) {
                    continue;
                }
                for col in 0..g.width as i32 {
                    let x = px + g.left + col;
                    if !xs.contains(&x) {
                        continue;
                    }
                    let i = (row * g.width as i32 + col) as usize;
                    let c = if g.rgba {
                        g.data.get(i * 4 + 3).copied().unwrap_or(0)
                    } else {
                        g.data.get(i).copied().unwrap_or(0)
                    };
                    let at = (y * side + x) as usize;
                    // Overlapping glyphs: the most coverage.
                    self.staging[at] = self.staging[at].max(c);
                }
            }
        }
    }

    fn upload(&mut self, queue: &wgpu::Queue, texture: &wgpu::Texture, rect: PageRect) {
        let mut rows = Vec::with_capacity((rect.width * rect.height) as usize);
        for y in rect.y..rect.bottom() {
            let at = (y * PAGE + rect.x) as usize;
            let row = &mut self.staging[at..at + rect.width as usize];
            rows.extend_from_slice(row);
            row.fill(0);
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: rect.x,
                    y: rect.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &rows,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(rect.width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: rect.width,
                height: rect.height,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &[u8] = include_bytes!("../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");

    #[test]
    fn families_are_read_from_a_css_stack() {
        assert_eq!(
            first_family("\"Iosevka Custom\", sans-serif"),
            "Iosevka Custom"
        );
        assert_eq!(first_family("system-ui"), "system-ui");
    }

    #[test]
    fn a_run_measures_and_draws_where_measured() {
        let mut r = CosmicRasterizer::new(&[FONT], false);
        let style = TextStyle {
            weight: 800,
            ..TextStyle::new("\"Iosevka Custom\", sans-serif", 20.0)
        };
        let m = r.measure("DITO", &style, 1.0);
        // The bundled Iosevka is monospaced, 0.6 em wide: four of them.
        assert!((m.advance - 48.0).abs() < 1.0, "{m:?}");
        // Capitals: ink above the baseline, within the advance.
        assert!(
            m.ink[1] < -10.0 && m.ink[3] <= 1.0 && m.ink[0] >= 0.0 && m.ink[2] <= 49.0,
            "{m:?}"
        );
        // The baseline sits within the 25 px line box.
        assert!(m.baseline > 12.0 && m.baseline < 25.0, "{m:?}");
        // Twice the scale, twice the size.
        let m2 = r.measure("DITO", &style, 2.0);
        assert!((m2.advance - 2.0 * m.advance).abs() < 1.0);
        let cell = PageRect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        r.draw("DITO", &style, 1.0, [10.0, 30.0], cell);
        let s = r.staging();
        let covered = |x0: i32, y0: i32, x1: i32, y1: i32| {
            let mut n = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    n += u32::from(s[(y as u32 * PAGE + x as u32) as usize] > 0);
                }
            }
            n
        };
        let [l, t, rr, b] = m.ink.map(|v| v as i32);
        let inside = covered(10 + l, 30 + t, 10 + rr, 30 + b);
        assert!(inside > 100, "{inside}");
        // Nothing drawn outside the measured ink.
        assert_eq!(covered(0, 0, 60, 60), inside);
    }
}
