//! Hidden / Sudden / Stealth: which drawn notes are visible.
//!
//! A port of StepMania 5.1's `ArrowGetPercentVisible`, `GetAlpha` and
//! `GetGlow` (`src/ArrowEffects.cpp` 1018–1161 at `825467b`): a note's
//! visibility depends only on its drawn distance from the receptor (after
//! speed and scroll effects), it is either fully shown or hidden (shown when
//! more than half visible), and it glows white around the switch. Notes past
//! the receptor are always shown (`m_bStealthPastReceptors` is off by
//! default). None of this touches judging.
//!
//! StepMania measures the lines in pixels on a 480-pixel screen with 64-pixel
//! arrows (7.5 arrows tall): centre line 160, fade distance 40. Our field is
//! 10 arrows tall in landscape (`ddi_render::scene::FieldGeometry`), so the
//! lines are scaled by 10/7.5 to sit at the same fraction of the screen:
//! one StepMania pixel is 1/48 of an arrow here. (Approximately: our receptors
//! sit 15 % down the screen against StepMania's 20 %.)

use serde::{Deserialize, Serialize};

/// DDR "APPEARANCE" / StepMania appearance mods.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Appearance {
    #[default]
    Visible,
    /// Notes vanish as they approach the receptor.
    Hidden,
    /// Notes appear only close to the receptor.
    Sudden,
    /// Both, with the lines spread apart as StepMania does.
    HiddenSudden,
    /// No notes are shown before the receptor.
    Stealth,
}

/// StepMania pixels per arrow height on our field.
const PIXELS_PER_ARROW: f32 = 48.0;
/// `CENTER_LINE_Y`, in arrow heights from the receptor.
const CENTER_LINE: f32 = 160.0 / PIXELS_PER_ARROW;
/// `FADE_DIST_Y`, in arrow heights.
const FADE_DIST: f32 = 40.0 / PIXELS_PER_ARROW;

/// StepMania's `SCALE`: maps `x` from `[l1, h1]` to `[l2, h2]`, unclamped.
fn scale(x: f32, l1: f32, h1: f32, l2: f32, h2: f32) -> f32 {
    l2 + (x - l1) * (h2 - l2) / (h1 - l1)
}

impl Appearance {
    fn hidden(self) -> bool {
        matches!(self, Appearance::Hidden | Appearance::HiddenSudden)
    }

    fn sudden(self) -> bool {
        matches!(self, Appearance::Sudden | Appearance::HiddenSudden)
    }

    /// `GetHiddenSudden()`: 1 when both are on, which spreads the lines.
    fn both(self) -> f32 {
        if self == Appearance::HiddenSudden {
            1.0
        } else {
            0.0
        }
    }

    /// `(start, end)` of the hidden fade: fully shown above `start`, gone
    /// below `end`.
    fn hidden_lines(self) -> (f32, f32) {
        let hs = self.both();
        (
            CENTER_LINE + FADE_DIST * scale(hs, 0.0, 1.0, 0.0, -0.25),
            CENTER_LINE + FADE_DIST * scale(hs, 0.0, 1.0, -1.0, -1.25),
        )
    }

    /// `(start, end)` of the sudden fade: gone above `start`, fully shown
    /// below `end`.
    fn sudden_lines(self) -> (f32, f32) {
        let hs = self.both();
        (
            CENTER_LINE + FADE_DIST * scale(hs, 0.0, 1.0, 1.0, 1.25),
            CENTER_LINE + FADE_DIST * scale(hs, 0.0, 1.0, 0.0, 0.25),
        )
    }

    /// `ArrowGetPercentVisible` for a note drawn `y` arrow heights above
    /// the receptor, in `0..=1`.
    pub fn percent_visible(self, y: f32) -> f32 {
        if y < 0.0 {
            return 1.0;
        }
        let mut adjust = 0.0;
        if self.hidden() {
            let (start, end) = self.hidden_lines();
            adjust += scale(y, start, end, 0.0, -1.0).clamp(-1.0, 0.0);
        }
        if self.sudden() {
            let (start, end) = self.sudden_lines();
            adjust += scale(y, start, end, -1.0, 0.0).clamp(-1.0, 0.0);
        }
        if self == Appearance::Stealth {
            adjust -= 1.0;
        }
        (1.0 + adjust).clamp(0.0, 1.0)
    }

    /// `(alpha, glow)` of a note at `y`: alpha is 0 or 1 (`GetAlpha`), glow
    /// peaks at 1.3 where the note switches (`GetGlow`).
    pub fn visibility(self, y: f32) -> (f32, f32) {
        if self == Appearance::Visible {
            return (1.0, 0.0);
        }
        let p = self.percent_visible(y);
        let alpha = if p > 0.5 { 1.0 } else { 0.0 };
        let glow = scale((p - 0.5).abs(), 0.0, 0.5, 1.3, 0.0).max(0.0);
        (alpha, glow)
    }

    /// The parts of `[lo, hi]` (arrow heights) where notes are shown, in
    /// ascending order (at most two, no allocation); for clipping hold
    /// bodies. Matches [`Appearance::visibility`]'s alpha except exactly on
    /// a boundary.
    pub fn visible_spans(self, lo: f32, hi: f32) -> impl Iterator<Item = (f32, f32)> {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let mut spans: [Option<(f32, f32)>; 2] = [None, None];
        // Shown before the receptor: y in [from, to].
        let (from, to) = match self {
            Appearance::Visible => {
                spans[0] = Some((lo, hi));
                return spans.into_iter().flatten();
            }
            Appearance::Stealth => (f32::INFINITY, f32::NEG_INFINITY),
            _ => {
                let from = if self.hidden() {
                    let (s, e) = self.hidden_lines();
                    (s + e) / 2.0
                } else {
                    0.0
                };
                let to = if self.sudden() {
                    let (s, e) = self.sudden_lines();
                    (s + e) / 2.0
                } else {
                    f32::INFINITY
                };
                (from, to)
            }
        };
        // Past the receptor: always shown.
        if lo < 0.0 {
            spans[0] = Some((lo, hi.min(0.0)));
        }
        let a = lo.max(from).max(0.0);
        let b = hi.min(to);
        if a < b {
            match &mut spans[0] {
                Some(first) if first.1 >= a => first.1 = b,
                _ => spans[1] = Some((a, b)),
            }
        }
        spans.into_iter().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn lines_match_stepmania_scaled_to_our_field() {
        // Hidden alone: fades between 160 and 120 px, gone below 140 px.
        let (s, e) = Appearance::Hidden.hidden_lines();
        assert!(close(s, 160.0 / 48.0) && close(e, 120.0 / 48.0));
        // Sudden alone: 200 → 160 px.
        let (s, e) = Appearance::Sudden.sudden_lines();
        assert!(close(s, 200.0 / 48.0) && close(e, 160.0 / 48.0));
        // Both: pushed 10 px apart each way.
        let (s, e) = Appearance::HiddenSudden.hidden_lines();
        assert!(close(s, 150.0 / 48.0) && close(e, 110.0 / 48.0));
        let (s, e) = Appearance::HiddenSudden.sudden_lines();
        assert!(close(s, 210.0 / 48.0) && close(e, 170.0 / 48.0));
    }

    #[test]
    fn hidden_hides_near_the_receptor_and_glows_at_the_switch() {
        let a = Appearance::Hidden;
        assert_eq!(a.visibility(6.0), (1.0, 0.0));
        assert_eq!(a.visibility(1.0).0, 0.0);
        assert_eq!(a.visibility(0.0).0, 0.0);
        // Past the receptor everything is shown again.
        assert_eq!(a.visibility(-0.5).0, 1.0);
        let mid = 140.0 / 48.0;
        assert_eq!(a.visibility(mid + 0.01).0, 1.0);
        assert_eq!(a.visibility(mid - 0.01).0, 0.0);
        assert!(a.visibility(mid).1 > 1.25);
    }

    #[test]
    fn sudden_shows_only_near_the_receptor() {
        let a = Appearance::Sudden;
        assert_eq!(a.visibility(1.0), (1.0, 0.0));
        assert_eq!(a.visibility(8.0).0, 0.0);
        let mid = 180.0 / 48.0;
        assert_eq!(a.visibility(mid - 0.01).0, 1.0);
        assert_eq!(a.visibility(mid + 0.01).0, 0.0);
    }

    #[test]
    fn stealth_hides_everything_before_the_receptor() {
        let a = Appearance::Stealth;
        for y in [0.0, 0.5, 3.0, 9.0] {
            assert_eq!(a.visibility(y), (0.0, 0.0));
        }
        assert_eq!(a.visibility(-0.1).0, 1.0);
        assert_eq!(Appearance::Visible.visibility(3.0), (1.0, 0.0));
    }

    #[test]
    fn spans_agree_with_alpha() {
        for a in [
            Appearance::Visible,
            Appearance::Hidden,
            Appearance::Sudden,
            Appearance::HiddenSudden,
            Appearance::Stealth,
        ] {
            let spans: Vec<_> = a.visible_spans(-2.0, 12.0).collect();
            let mut y = -2.0f32;
            while y <= 12.0 {
                let inside = spans.iter().any(|&(lo, hi)| y >= lo && y <= hi);
                let on_edge = spans
                    .iter()
                    .any(|&(lo, hi)| (y - lo).abs() < 2e-3 || (y - hi).abs() < 2e-3);
                if !on_edge {
                    assert_eq!(inside, a.visibility(y).0 == 1.0, "{a:?} at {y}");
                }
                y += 0.01;
            }
        }
        let spans: Vec<_> = Appearance::Visible.visible_spans(3.0, 1.0).collect();
        assert_eq!(spans, vec![(1.0, 3.0)]);
    }
}
