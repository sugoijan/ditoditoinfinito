//! The tiers' colors: the table a judgment's color is read from (the
//! play's word, the built-in arrows' receptor flash, the results' rows
//! and timing histogram, a stage's standings bar and its miniatures'
//! flashes, Options' preview, Infinito's hit effects).
//!
//! Two sets: StepMania's, which every borrowed ruleset has always shown
//! (Simply Love's, the FA+ white Fantastic among them), and DDI's own
//! (`docs/plans/ddi-ruleset.md`, "Hit effects": DDR's order, white for
//! the best, then yellow, green, blue, orange and red for a miss). A
//! ruleset's set travels with its names ([`super::JudgeNames::palette`]).

use serde::{Deserialize, Serialize};

use super::Judgement;

/// Which set of tier colors a ruleset's judgments are shown in.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Palette {
    /// The colors every ruleset but DDI shows (StepMania's themes').
    #[default]
    StepMania,
    /// DDI's own.
    Ddi,
}

/// A color as RGBA in 0..1, sRGB-encoded (as the renderer's and the
/// theme's colors are written).
pub type Rgba = [f32; 4];

/// `#rrggbb` as an [`Rgba`].
pub const fn rgb(hex: u32) -> Rgba {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    ]
}

/// A set of tier colors, best first.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TierColors {
    /// A hit inside the ruleset's W0 window (FA+'s blue Fantastic, DDI's
    /// Absolute), and a W1 where the ruleset has no such window or it is
    /// not known (a receptor's flash).
    pub w0: Rgba,
    /// W1 to Miss, by [`Judgement::index`]; W1's is a hit outside the W0
    /// window.
    pub tiers: [Rgba; 6],
    /// A stray press counted against the play (DDI's Boo).
    pub boo: Rgba,
}

/// StepMania's set, exactly as the renderer has always drawn it: a cyan
/// best tier (FA+'s blue Fantastic) with Simply Love's white one outside
/// FA+'s window, yellow, green, violet, orange, red. The theme's
/// `Judgement.W1` … `Judgement.Miss` keep their own file's values, a
/// shade apart from these in places, as they always were: neither moves.
pub const STEPMANIA: TierColors = TierColors {
    w0: [0.55, 0.95, 1.0, 1.0],
    tiers: [
        [0.96, 0.95, 0.97, 1.0],
        [1.0, 0.9, 0.35, 1.0],
        [0.45, 0.95, 0.45, 1.0],
        [0.75, 0.5, 1.0, 1.0],
        [1.0, 0.55, 0.3, 1.0],
        [1.0, 0.3, 0.3, 1.0],
    ],
    boo: rgb(0xff5c9e),
};

/// DDI's set (round 4, 2026-11-13; awaiting the maintainer's eye): W0
/// and W1 white (W0 the brighter), W2 yellow, W3 green, W4 blue, W5
/// orange ("nearly a miss": next to Miss's red and away from the Boo's
/// pink; a violet W5 would match W4's blue for a red-green deficient
/// eye), Miss red, the Boo's pink as before. Each at least 5.9:1 against
/// the field's ground (`docs/plans/ddi-ruleset.md`, "Hit effects"). The
/// one table for DDI: the renderer, the theme (`Judgement.Ddi.*`, made
/// from it) and Infinito's hit effects (`NOTESKIN:GetJudgmentColor`)
/// read it.
pub const DDI: TierColors = TierColors {
    w0: rgb(0xffffff),
    tiers: [
        rgb(0xe2e4ee),
        rgb(0xffe14a),
        rgb(0x45eb7f),
        rgb(0x4f9dff),
        rgb(0xff9633),
        rgb(0xff3d3d),
    ],
    boo: rgb(0xff5c9e),
};

/// The violet W5 shown beside the orange one when the set was chosen
/// (for the comparison sheet; never shown in play).
pub const DDI_W5_VIOLET: Rgba = rgb(0xb77bff);

impl Palette {
    /// The set's colors.
    pub fn colors(self) -> &'static TierColors {
        match self {
            Palette::StepMania => &STEPMANIA,
            Palette::Ddi => &DDI,
        }
    }

    pub fn is_stepmania(&self) -> bool {
        *self == Palette::StepMania
    }
}

impl TierColors {
    /// The color of a hit of tier `j`: `w0` `Some(true)` inside the
    /// ruleset's W0 window, `Some(false)` a W1 outside it, `None` where
    /// the ruleset has none (or it is not known: a receptor's flash).
    pub fn color(&self, j: Judgement, w0: Option<bool>) -> Rgba {
        match (j, w0) {
            (Judgement::W1, Some(false)) => self.tiers[0],
            (Judgement::W1, _) => self.w0,
            (j, _) => self.tiers[j.index()],
        }
    }

    /// The colors by their names after `prefix` (`Judgement.Ddi.`):
    /// `W0`, `W1` … `W5`, `Miss`, `Boo`.
    pub fn named(&self, prefix: &str) -> Vec<(String, Rgba)> {
        let mut v = vec![(format!("{prefix}W0"), self.w0)];
        for (i, c) in self.tiers.iter().enumerate() {
            let name = if i == 5 {
                "Miss".to_string()
            } else {
                format!("W{}", i + 1)
            };
            v.push((format!("{prefix}{name}"), *c));
        }
        v.push((format!("{prefix}Boo"), self.boo));
        v
    }
}

/// `c` as `#rrggbb`.
pub fn hex(c: Rgba) -> String {
    let b = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The StepMania set is the colors every borrowed ruleset showed
    /// before DDI had its own (the renderer's constants as they were).
    #[test]
    fn stepmania_s_colors_are_as_they_were() {
        let c = Palette::StepMania.colors();
        assert_eq!(c.color(Judgement::W1, None), [0.55, 0.95, 1.0, 1.0]);
        assert_eq!(c.color(Judgement::W1, Some(true)), [0.55, 0.95, 1.0, 1.0]);
        assert_eq!(c.color(Judgement::W1, Some(false)), [0.96, 0.95, 0.97, 1.0]);
        assert_eq!(c.color(Judgement::W2, None), [1.0, 0.9, 0.35, 1.0]);
        assert_eq!(c.color(Judgement::W3, None), [0.45, 0.95, 0.45, 1.0]);
        assert_eq!(c.color(Judgement::W4, None), [0.75, 0.5, 1.0, 1.0]);
        assert_eq!(c.color(Judgement::W5, None), [1.0, 0.55, 0.3, 1.0]);
        assert_eq!(c.color(Judgement::Miss, None), [1.0, 0.3, 0.3, 1.0]);
        assert_eq!(hex(c.boo), "#ff5c9e");
    }

    fn hue(c: Rgba) -> (f32, f32) {
        let [r, g, b] = [c[0], c[1], c[2]];
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let h = if max == min {
            0.0
        } else if max == r {
            60.0 * ((g - b) / (max - min))
        } else if max == g {
            60.0 * ((b - r) / (max - min)) + 120.0
        } else {
            60.0 * ((r - g) / (max - min)) + 240.0
        };
        (h.rem_euclid(360.0), (max - min) / max)
    }

    /// DDI's: white, white, yellow, green, blue, orange, red, by hue.
    #[test]
    fn ddi_s_tiers_go_white_yellow_green_blue_orange_red() {
        let c = Palette::Ddi.colors();
        // W0 and W1 white: barely saturated, W0 the brighter.
        assert!(hue(c.w0).1 < 0.05 && hue(c.tiers[0]).1 < 0.1);
        assert!(c.w0[..3].iter().sum::<f32>() > c.tiers[0][..3].iter().sum::<f32>());
        let between = |c, lo: f32, hi: f32| {
            let (h, s) = hue(c);
            s > 0.5 && (lo..hi).contains(&h)
        };
        assert!(between(c.tiers[1], 45.0, 60.0), "W2 yellow");
        assert!(between(c.tiers[2], 120.0, 150.0), "W3 green");
        assert!(between(c.tiers[3], 205.0, 225.0), "W4 blue");
        assert!(between(c.tiers[4], 20.0, 40.0), "W5 orange");
        assert!(between(c.tiers[5], 0.0, 5.0), "Miss red");
        assert!(between(DDI_W5_VIOLET, 260.0, 280.0));
        // The Boo as it was.
        assert_eq!(c.boo, STEPMANIA.boo);
        assert_eq!(c.color(Judgement::W1, Some(true)), c.w0);
        assert_eq!(c.color(Judgement::W1, Some(false)), c.tiers[0]);
    }

    /// Every DDI color reads on the field's ground (#05020d) at 4.5:1 or
    /// more (WCAG's ratio for text).
    #[test]
    fn ddi_s_colors_read_on_the_ground() {
        let lum = |c: Rgba| {
            let l = |v: f32| {
                let v = f64::from(v);
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * l(c[0]) + 0.7152 * l(c[1]) + 0.0722 * l(c[2])
        };
        let ground = lum(rgb(0x05020d));
        let c = Palette::Ddi.colors();
        for rgba in std::iter::once(c.w0).chain(c.tiers).chain([c.boo]) {
            let ratio = (lum(rgba) + 0.05) / (ground + 0.05);
            assert!(ratio >= 4.5, "{} at {ratio:.1}", hex(rgba));
        }
    }

    #[test]
    fn the_named_colors_are_the_theme_s_names() {
        let named = DDI.named("Judgement.Ddi.");
        let names: Vec<&str> = named.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "Judgement.Ddi.W0",
                "Judgement.Ddi.W1",
                "Judgement.Ddi.W2",
                "Judgement.Ddi.W3",
                "Judgement.Ddi.W4",
                "Judgement.Ddi.W5",
                "Judgement.Ddi.Miss",
                "Judgement.Ddi.Boo"
            ]
        );
    }
}
