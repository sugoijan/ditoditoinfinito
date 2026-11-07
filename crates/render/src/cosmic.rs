//! Text runs drawn by cosmic-text (`docs/plans/ui-core.md`, P4): the
//! [`RunRasterizer`] of desktop, embedded targets and native tests. The
//! web draws runs with the browser instead (`crates/web/src/web/text.rs`); this
//! one stays buildable there, as the way back should the browser's text
//! ever be dropped.
//!
//! Fonts are the bundled ones given, and the system's when asked for
//! (cosmic-text falls back per script for glyphs a font lacks). A style's
//! family is a CSS font stack, resolved as a browser does: each grapheme
//! is set in the first family named in the stack that has it (the theme's
//! face for Latin, the language's for its script, `ddi_l10n::font_stack`),
//! the rest in the first family with cosmic-text's fallback; CSS's generic
//! names stand for cosmic-text's and end the search. The UI's language
//! chooses the system's faces for unified Han characters
//! ([`RunRasterizer::set_language`]: Japanese forms in Japanese).

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use ddi_actor::text::{RunMetrics, TextStyle};
use glyphon::cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent, Weight,
    fontdb,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::atlas::PageRect;
use crate::text_atlas::{PAGE, RunRasterizer};

/// A bundled face: the family styles name it by (as the web registers a
/// `FontFace` under its name; a face's own name may carry its weight,
/// "Iosevka Term Semibold"), its file, and the CSS weight it stands for
/// where that is not its own (Noto Sans CJK's Medium for the theme's 600).
#[derive(Clone, Copy, Debug)]
pub struct Bundled<'a> {
    pub family: &'a str,
    pub bytes: &'a [u8],
    pub weight: Option<u16>,
}

pub struct CosmicRasterizer {
    fonts: FontSystem,
    swash: SwashCache,
    /// The staging page: coverage, one byte a texel.
    staging: Vec<u8>,
    /// The locale the system's fonts were first matched for: the one kept
    /// for languages that do not choose Han's forms.
    system_locale: String,
    /// The face a family gives a weight and slant, as looked up (a few:
    /// the families stacks name).
    matched: Vec<(String, u16, bool, Option<fontdb::ID>)>,
    /// The code points each face looked at has.
    coverage: HashMap<fontdb::ID, HashSet<u32>>,
}

/// One glyph's coverage and where its top left goes, from the pen.
struct Glyph {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    /// Coverage (a mask), or RGBA whose alpha is taken (a color glyph).
    data: Vec<u8>,
    rgba: bool,
}

impl CosmicRasterizer {
    /// With the `bundled` faces, each under the family name given and at
    /// the weight given, and the system's with `system`.
    pub fn with_families(bundled: &[Bundled], system: bool) -> CosmicRasterizer {
        let mut r = CosmicRasterizer::new(&[], system);
        let db = r.fonts.db_mut();
        for face in bundled {
            let source = fontdb::Source::Binary(std::sync::Arc::new(face.bytes.to_vec()));
            for id in db.load_font_source(source) {
                let Some(mut info) = db.face(id).cloned() else {
                    continue;
                };
                db.remove_face(id);
                info.families = vec![(
                    face.family.to_string(),
                    fontdb::Language::English_UnitedStates,
                )];
                if let Some(w) = face.weight {
                    info.weight = fontdb::Weight(w);
                }
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
            system_locale: fonts.locale().to_string(),
            fonts,
            swash: SwashCache::new(),
            staging: vec![0; (PAGE * PAGE) as usize],
            matched: Vec::new(),
            coverage: HashMap::new(),
        }
    }

    /// The face `family` gives `weight` and slant.
    fn face(&mut self, family: &str, weight: u16, italic: bool) -> Option<fontdb::ID> {
        if let Some(m) = self
            .matched
            .iter()
            .find(|m| m.0 == family && m.1 == weight && m.2 == italic)
        {
            return m.3;
        }
        let id = self.fonts.db().query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight(weight),
            stretch: fontdb::Stretch::Normal,
            style: if italic {
                fontdb::Style::Italic
            } else {
                fontdb::Style::Normal
            },
        });
        self.matched.push((family.to_string(), weight, italic, id));
        id
    }

    /// Whether `family`'s face at `weight` and slant has `c`.
    fn covers(&mut self, family: &str, weight: u16, italic: bool, c: char) -> bool {
        let id = self.face(family, weight, italic);
        let db = self.fonts.db();
        let Some(id) = id else {
            return false;
        };
        self.coverage
            .entry(id)
            .or_insert_with(|| {
                db.with_face_data(id, |data, index| {
                    let mut set = HashSet::new();
                    let cmap = ttf_parser::Face::parse(data, index)
                        .ok()
                        .and_then(|f| f.tables().cmap);
                    for table in cmap.into_iter().flat_map(|c| c.subtables) {
                        if table.is_unicode() {
                            table.codepoints(|cp| {
                                set.insert(cp);
                            });
                        }
                    }
                    set
                })
                .unwrap_or_default()
            })
            .contains(&u32::from(c))
    }

    /// `text` in spans, each with the index in `families` (a stack's)
    /// of the family it is set in: per grapheme, the first named family
    /// having it, else the first family (cosmic-text then falls back).
    fn spans(
        &mut self,
        text: &str,
        families: &[String],
        weight: u16,
        italic: bool,
    ) -> Vec<(Range<usize>, usize)> {
        let mut out: Vec<(Range<usize>, usize)> = Vec::new();
        for (at, g) in text.grapheme_indices(true) {
            let c = g.chars().next().unwrap_or(' ');
            let mut pick = 0;
            for (i, f) in families.iter().enumerate() {
                if generic(f).is_some() {
                    break;
                }
                if self.covers(f, weight, italic, c) {
                    pick = i;
                    break;
                }
            }
            let end = at + g.len();
            match out.last_mut() {
                Some((r, i)) if *i == pick => r.end = end,
                _ => out.push((at..end, pick)),
            }
        }
        out
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
        let families = families(&style.family);
        let spans = if families.len() > 1 {
            self.spans(text, &families, style.weight, style.italic)
        } else {
            Vec::new()
        };
        let first = families.first().map_or("", String::as_str);
        let mut buffer = Buffer::new(&mut self.fonts, metrics);
        buffer.set_size(None, None);
        if spans.len() > 1 {
            buffer.set_rich_text(
                spans
                    .iter()
                    .map(|(r, i)| (&text[r.clone()], attrs(&families[*i], style))),
                &attrs(first, style),
                Shaping::Advanced,
                None,
            );
        } else {
            let family = spans.first().map_or(first, |s| families[s.1].as_str());
            buffer.set_text(text, &attrs(family, style), Shaping::Advanced, None);
        }
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

/// The families of a CSS font stack, unquoted, in order (one empty name
/// at least).
fn families(stack: &str) -> Vec<String> {
    stack
        .split(',')
        .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        .collect()
}

/// How a span in family `name` is set in `style`.
fn attrs<'a>(name: &'a str, style: &TextStyle) -> Attrs<'a> {
    let attrs = Attrs::new()
        .family(generic(name).unwrap_or(Family::Name(name)))
        .weight(Weight(style.weight))
        .style(if style.italic {
            Style::Italic
        } else {
            Style::Normal
        });
    if style.tracking != 0 {
        attrs.letter_spacing(f32::from(style.tracking) / 1000.0)
    } else {
        attrs
    }
}

/// cosmic-text's family for a CSS generic name.
fn generic(name: &str) -> Option<Family<'static>> {
    Some(match name {
        "sans-serif" | "system-ui" | "ui-sans-serif" => Family::SansSerif,
        "serif" | "ui-serif" => Family::Serif,
        "monospace" | "ui-monospace" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        _ => return None,
    })
}

/// The locale cosmic-text matches the system's faces for unified Han
/// characters by, for the UI's language `tag` (`None`: not one choosing
/// them).
fn han_locale(tag: &str) -> Option<&'static str> {
    let mut parts = tag.split(['-', '_']);
    let rest: Vec<&str> = parts.clone().skip(1).collect();
    Some(match parts.next()? {
        "ja" => "ja",
        "ko" => "ko",
        "zh" if rest.iter().any(|p| matches!(*p, "HK" | "MO")) => "zh-HK",
        "zh" if rest.iter().any(|p| matches!(*p, "TW" | "Hant")) => "zh-TW",
        "zh" => "zh-CN",
        _ => return None,
    })
}

impl RunRasterizer for CosmicRasterizer {
    fn set_language(&mut self, tag: &str) {
        let locale = han_locale(tag).map_or_else(|| self.system_locale.clone(), str::to_string);
        if self.fonts.locale() == locale {
            return;
        }
        // The same faces (and ids), matched for the new locale.
        let none = FontSystem::new_with_locale_and_db(String::new(), fontdb::Database::new());
        let (_, db) = std::mem::replace(&mut self.fonts, none).into_locale_and_db();
        self.fonts = FontSystem::new_with_locale_and_db(locale, db);
    }

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

    const FONT: &[u8] = include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");

    const TERM: &[u8] = include_bytes!("../../../assets/fonts/IosevkaTerm-Regular-latin.ttf");
    const NOTO: &[u8] = include_bytes!("../../../assets/fonts/NotoSansCJKjp-Regular-ja.otf");
    const NOTO_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/NotoSansCJKjp-Medium-ja.otf");

    #[test]
    fn families_are_read_from_a_css_stack() {
        assert_eq!(
            families("\"Iosevka Custom\", sans-serif"),
            ["Iosevka Custom", "sans-serif"]
        );
        assert_eq!(families("system-ui"), ["system-ui"]);
    }

    /// Iosevka Term and Noto Sans CJK JP's cuts, no system faces.
    fn bundled() -> CosmicRasterizer {
        let face = |family, bytes, weight| Bundled {
            family,
            bytes,
            weight,
        };
        CosmicRasterizer::with_families(
            &[
                face("Iosevka Term", TERM, None),
                face("Noto Sans CJK JP", NOTO, None),
                face("Noto Sans CJK JP", NOTO_MEDIUM, Some(600)),
            ],
            false,
        )
    }

    #[test]
    fn each_grapheme_takes_the_first_family_of_the_stack_that_has_it() {
        let mut r = bundled();
        let stack = families("Iosevka Term, Noto Sans CJK JP, system-ui, sans-serif");
        // Latin and digits in Iosevka, kana and kanji in Noto, a kanji
        // the cut lacks in the first family (cosmic-text falls back).
        let text = "A言語を1鬱";
        let spans = r.spans(text, &stack, 400, false);
        let named: Vec<(&str, &str)> = spans
            .iter()
            .map(|(range, i)| (&text[range.clone()], stack[*i].as_str()))
            .collect();
        assert_eq!(
            named,
            [
                ("A", "Iosevka Term"),
                ("言語を", "Noto Sans CJK JP"),
                ("1鬱", "Iosevka Term")
            ]
        );
        // A generic name ends the search.
        let generic_first = families("sans-serif, Noto Sans CJK JP");
        assert!(
            r.spans("言", &generic_first, 400, false)
                .iter()
                .all(|s| s.1 == 0)
        );
        // The theme's SemiBold finds Noto's Medium, registered at 600.
        assert!(r.covers("Noto Sans CJK JP", 600, false, '言'));
        let medium = r.face("Noto Sans CJK JP", 600, false);
        let regular = r.face("Noto Sans CJK JP", 400, false);
        assert!(medium.is_some() && medium != regular);
    }

    #[test]
    fn japanese_is_set_a_full_em_beside_half_em_latin() {
        let mut r = bundled();
        let style = TextStyle::new("Iosevka Term, Noto Sans CJK JP", 20.0);
        let m = r.measure("AB言語", &style, 1.0);
        // Iosevka Term is half an em wide, Noto's kanji a whole one.
        assert!((m.advance - 60.0).abs() < 1.0, "{m:?}");
    }

    #[test]
    fn the_language_chooses_han_s_forms() {
        assert_eq!(han_locale("ja"), Some("ja"));
        assert_eq!(han_locale("ja-JP"), Some("ja"));
        assert_eq!(han_locale("zh-Hant-TW"), Some("zh-TW"));
        assert_eq!(han_locale("zh"), Some("zh-CN"));
        assert_eq!(han_locale("en"), None);
        let mut r = bundled();
        let system = r.fonts.locale().to_string();
        r.set_language("ja");
        assert_eq!(r.fonts.locale(), "ja");
        // The same faces after.
        assert!(r.covers("Noto Sans CJK JP", 400, false, '言'));
        r.set_language("en");
        assert_eq!(r.fonts.locale(), system);
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
