//! Text as actors use it: a run of text in a style, measured by whatever
//! draws it (`docs/plans/ui-core.md`, P4). On the web the browser measures
//! and draws runs (Canvas2D), on desktop cosmic-text does; both answer
//! [`TextMeasure`], so layout here never depends on the platform.
//!
//! Sizes and metrics are in the units of the screen the actor lives in
//! (virtual pixels); the engine measures at the physical size it will draw
//! at and scales back, so a run measures the same at any device pixel
//! ratio up to rounding.

use std::sync::Arc;

/// A shallow cylindrical bend, applied after shaping without changing the
/// text cache or its advance. Coordinates are relative to the run's pen;
/// y grows down. The offset is zero at x=0 and has its turning point at
/// `center`. A negative quadratic makes a keycap's legend dip in the middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextCurve {
    pub center: f32,
    pub quadratic: f32,
}

impl TextCurve {
    pub fn offset(self, x: f32) -> f32 {
        self.quadratic * x * (x - 2.0 * self.center)
    }

    /// Corners of a strip: top left, top right, bottom right, bottom left.
    pub fn corners(self, [left, top, right, bottom]: [f32; 4]) -> [[f32; 2]; 4] {
        let l = self.offset(left);
        let r = self.offset(right);
        [
            [left, top + l],
            [right, top + r],
            [right, bottom + r],
            [left, bottom + l],
        ]
    }
}

/// How a run is set. Two runs with equal styles and text draw alike.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextStyle {
    /// A font stack as CSS writes it (`"Iosevka Custom", sans-serif`):
    /// the engine resolves it, falling back for glyphs a font lacks.
    pub family: Arc<str>,
    /// The size in 1/64 of a virtual pixel, so styles can be hashed.
    pub size_64: u32,
    /// CSS weight, 100–900.
    pub weight: u16,
    pub italic: bool,
    /// Extra space between letters, thousandths of an em (CSS's
    /// `letter-spacing`): our extension for the theme's small capitals.
    pub tracking: i16,
}

impl TextStyle {
    pub fn new(family: impl Into<Arc<str>>, size: f32) -> TextStyle {
        TextStyle {
            family: family.into(),
            size_64: (size * 64.0).round().max(0.0) as u32,
            weight: 400,
            italic: false,
            tracking: 0,
        }
    }

    pub fn size(&self) -> f32 {
        self.size_64 as f32 / 64.0
    }

    /// The height of a line: our own metric, since a font's box covers
    /// only its own glyphs and fallback glyphs overflow it.
    pub fn line_height(&self) -> f32 {
        self.size() * LINE_HEIGHT
    }
}

/// Line height as a multiple of the size.
pub const LINE_HEIGHT: f32 = 1.25;

/// What a measured run occupies, relative to its pen position on the
/// baseline (y grows downwards).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RunMetrics {
    /// How far the pen moves.
    pub advance: f32,
    /// The drawn pixels' box, `[left, top, right, bottom]`; ink can reach
    /// past the advance on either side.
    pub ink: [f32; 4],
    /// From the top of the line box ([`TextStyle::line_height`]) to the
    /// baseline.
    pub baseline: f32,
}

/// Measures runs. Implementations cache; measuring must be synchronous
/// and must not need the run to have been drawn.
pub trait TextMeasure {
    fn measure(&mut self, text: &str, style: &TextStyle) -> RunMetrics;
}

/// A measurer for tests: every character is `0.5 × size` wide, ink fills
/// the advance from the baseline up to the size, the baseline sits at
/// 0.8 of the line height.
#[derive(Clone, Copy, Debug, Default)]
pub struct FixedMeasure;

impl TextMeasure for FixedMeasure {
    fn measure(&mut self, text: &str, style: &TextStyle) -> RunMetrics {
        let size = style.size();
        let advance = text.chars().count() as f32 * size * 0.5;
        RunMetrics {
            advance,
            ink: [0.0, -size, advance, 0.0],
            baseline: style.line_height() * 0.8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_round_to_a_sixty_fourth() {
        let s = TextStyle::new("sans-serif", 18.01);
        assert_eq!(s.size_64, 1153);
        assert_eq!(TextStyle::new("sans-serif", 18.0).size(), 18.0);
        assert_eq!(TextStyle::new("sans-serif", 16.0).line_height(), 20.0);
    }

    #[test]
    fn fixed_measure_counts_characters() {
        let m = FixedMeasure.measure("ジャン!", &TextStyle::new("sans-serif", 20.0));
        assert_eq!(m.advance, 40.0);
        assert_eq!(m.ink, [0.0, -20.0, 40.0, 0.0]);
    }
}
