//! Chart-relative skill labels and radii shared by qualification and song select.

/// The DDI estimate's radar (round Z3, the maintainer: "set a minimum
/// value of 1 for each skill and grow everything proportionally to the
/// difficulty"): each skill's value `1 + e`, `e` how much it stands out,
/// 0 to 1 (since round Z4 its percentile among charts of similar
/// difficulty, [`skill_standing`]), so nothing below the floor; a spoke `base(decimal) × (1 + e ×` [`GROWTH`]`)` of the
/// radius, `base` rising linearly from [`BASE_AT_1`] at 1 to
/// [`BASE_AT_19`] at 19. A chart with no costs is a regular octagon whose
/// size says its difficulty; the longest spoke there can be (19, a cost
/// without end) reaches the outer ring.
///
/// The cost, in points of Accuracy, at which a skill without a table
/// (Handstream) is halfway to its most: a typical chart's Stream costs
/// about 2.
pub const COST_HALF: f32 = 2.0;
/// How far past the base a skill's spoke reaches at its most: six tenths
/// of it, a bulge rather than a spike, and room enough for the base to
/// grow 2.8 times from 1 to 19.
pub const GROWTH: f32 = 0.6;
/// The base octagon at 1 and at 19, as fractions of the radius: 0.22
/// keeps the easiest chart's octagon a clear octagon (14 px at the 1280 ×
/// 720 radar's 64) inside its labels; at 19, `1 / (1 + GROWTH)`, so the
/// longest spoke ends at the outer ring.
pub const BASE_AT_1: f32 = 0.22;
pub const BASE_AT_19: f32 = 1.0 / (1.0 + GROWTH);

/// The base octagon's size for an estimate of `decimal` (1 to 19), as a
/// fraction of the radius.
pub fn estimate_base(decimal: f32) -> f32 {
    let t = ((decimal - 1.0) / 18.0).clamp(0.0, 1.0);
    BASE_AT_1 + (BASE_AT_19 - BASE_AT_1) * t
}

/// How the DDI estimate's radar reads a skill (round Z4, the maintainer:
/// "do some normalization so that the apparent skill level is not
/// absolute but relative to the distribution"), used by song select and the lower-band Stream qualification.
pub const SPOKE_RULE: &str = "relative-profile-v0";

/// At or under this many points of Accuracy a skill costs nothing to
/// speak of: its spoke is the base.
pub const COST_FLOOR: f32 = 0.1;

/// Where `cost` stands among `knots` (the costs at
/// [`crate::radar_table::KNOT_PCT`]'s percentiles): 0 at or under the
/// first, 1 at or past the last, linear between knots.
fn percentile(knots: &[f32; 8], cost: f32) -> f32 {
    use crate::radar_table::KNOT_PCT;
    if cost <= knots[0] {
        return 0.0;
    }
    for k in 1..8 {
        if cost < knots[k] {
            let (a, b) = (knots[k - 1], knots[k]);
            let t = if b > a { (cost - a) / (b - a) } else { 1.0 };
            return KNOT_PCT[k - 1] + (KNOT_PCT[k] - KNOT_PCT[k - 1]) * t;
        }
    }
    1.0
}

/// How much skill `k` (`ddi_engine::cpu::SKILL_NAMES`' order) of a chart
/// estimated at `decimal` stands out, 0 to 1 ([`SPOKE_RULE`]): 0 where it
/// costs [`COST_FLOOR`] or less; else its percentile among charts of
/// similar difficulty where it costs anything
/// ([`crate::radar_table::BANDS`]), the two bands about `decimal` blended
/// by it (the first band's below the first center, the last's past the
/// last), a band without enough charts reading every chart's
/// ([`crate::radar_table::ALL`]); a skill with no table at all
/// (Handstream) as round Z3 had it, `cost / (cost + COST_HALF)`.
pub fn skill_standing(decimal: f32, k: usize, cost: f32) -> f32 {
    use crate::radar_table::{ALL, BAND_CENTERS, BANDS};
    let cost = cost.max(0.0);
    if cost <= COST_FLOOR {
        return 0.0;
    }
    let Some(all) = ALL[k] else {
        return cost / (cost + COST_HALF);
    };
    let at = |band: usize| percentile(BANDS[band][k].as_ref().unwrap_or(&all), cost);
    let last = BAND_CENTERS.len() - 1;
    if decimal <= BAND_CENTERS[0] {
        return at(0);
    }
    if decimal >= BAND_CENTERS[last] {
        return at(last);
    }
    let i = BAND_CENTERS
        .iter()
        .rposition(|c| *c <= decimal)
        .unwrap_or(0)
        .min(last - 1);
    let t = (decimal - BAND_CENTERS[i]) / (BAND_CENTERS[i + 1] - BAND_CENTERS[i]);
    at(i) + (at(i + 1) - at(i)) * t
}

/// The DDI estimate's spokes, as fractions of the radius, for a chart of
/// `decimal` whose skills cost `costs` points of Accuracy: the base
/// octagon of its difficulty, each skill past it by how much it stands
/// out ([`skill_standing`]).
pub fn estimate_spokes(decimal: f32, costs: &[f32; 8]) -> Vec<f32> {
    costs
        .iter()
        .enumerate()
        .map(|(k, c)| spoke_radius(decimal, k, *c))
        .collect()
}

/// A DDI estimate's spoke's number, consistent with its length: the
/// skill's level on the difficulty scale, `decimal × (1 + e × GROWTH) /
/// (1 + 0.5 × GROWTH)`, so a skill as demanding as usual for charts this
/// hard reads the chart's own estimate, a standout up to about a quarter
/// above it, an absent one about a quarter below. The one place a
/// spoke's number is made: to show the costs again, return `cost`.
pub fn spoke_number(decimal: f32, k: usize, cost: f32) -> f32 {
    let e = skill_standing(decimal, k, cost);
    decimal * (1.0 + e * GROWTH) / (1.0 + 0.5 * GROWTH)
}

fn spoke_radius(decimal: f32, k: usize, cost: f32) -> f32 {
    estimate_base(decimal) * (1.0 + skill_standing(decimal, k, cost) * GROWTH)
}

/// Bracketing CPU levels and interpolation fraction at the full-precision chart estimate.
pub(super) fn matched_levels(decimal: f64) -> (usize, usize, f64) {
    let level = (decimal / 19.0 * 10.0).clamp(1.0, 10.0) - 1.0;
    let i = level.floor() as usize;
    (i, (i + 1).min(9), level - i as f64)
}

/// Sensitivity at the chart's matched level under the frozen ±1 Accuracy pp test.
pub(super) fn stream_display_stable(decimal: f64, means: &[[f64; 9]; 10]) -> bool {
    let (i, j, f) = matched_levels(decimal);
    let cost = (1.0 - f) * means[i][1] + f * means[j][1];
    let hundredth = |x: f64| ((x * 100.0).round() / 100.0) as f32;
    let decimal = hundredth(decimal);
    let display = |cost: f64| {
        let cost = hundredth(cost.max(0.0));
        // Formatting an f32 with one decimal uses ties-to-even. Scale in f64
        // so the multiplication doesn't round before choosing the displayed tenth.
        let tenths = (f64::from(spoke_number(decimal, 0, cost)) * 10.0).round_ties_even() as i32;
        (tenths, f64::from(spoke_radius(decimal, 0, cost)))
    };
    let (label, radius) = display(cost);
    [cost - 1.0, cost + 1.0].into_iter().all(|perturbed| {
        let (other_label, other_radius) = display(perturbed);
        (label - other_label).abs() <= 15 && (radius - other_radius).abs() <= 0.08
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_band_stream_uses_the_displayed_change_at_the_matched_level() {
        let mut means = [[0.0; 9]; 10];
        means[9][1] = 6.154108047485352; // Brandish Headless
        assert!(!stream_display_stable(19.0, &means));
        means[9][1] = 8.04; // 1.5 displayed points is included
        assert!(stream_display_stable(19.0, &means));
        means[9][1] = 17.38; // Glimpse of Us
        assert!(stream_display_stable(19.0, &means));
        // At 18.05 the level is halfway between rows 8 and 9. The final
        // row alone would permit a number on this steep part of the display.
        means[8][1] = 2.0;
        means[9][1] = 10.4;
        assert!(!stream_display_stable(18.05, &means));
    }
}
