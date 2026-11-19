//! Frozen whole-chart keyboard 4K estimates under DDI Normal Practice at 1×.
//! Call [`estimate`] with an imported chart. Run outside the play loop.
mod coefficients;
mod features;
mod network;
mod prior;

use ddi_chart::{ChartDigest, Song};
use ddi_engine::cpu::{
    SKILL_NAMES,
    teacher_v1::{Target, TargetStatus},
};
use serde::Serialize;
use std::array;

pub const ESTIMATOR: &str = "single-pass-rating-v1";
pub const SCALE: &str = "ddi-ddr19-prototype-v1";

#[derive(Debug, Serialize)]
pub struct Estimate {
    pub estimator: &'static str,
    pub scale: &'static str,
    pub chart_digest: ChartDigest,
    /// The decimal estimate and its round-half-up display level.
    pub decimal: f64,
    pub level: u8,
    /// Qualification for `decimal` and `level`: 48+ heads, at most 1,200
    /// source notes, 15–180 seconds and no lifts.
    pub within_scalar_domain: bool,
    /// Qualification for the score curve and skill readouts: the scalar
    /// conditions with at most 900 source notes.
    pub within_studied_domain: bool,
    /// Qualification for the song-select profile at the chart's decimal-matched
    /// CPU level. Extends the 900-note domain to 1,200 on mine-free charts.
    pub within_matched_profile_domain: bool,
    /// At balanced CPU levels 1–10: mean raw Accuracy, then eight skill costs
    /// (balanced minus weakened Accuracy in percentage points, SKILL_NAMES order).
    pub means: [[f64; 9]; 10],
    /// Calibrated against the expected score curve.
    pub expected: Option<Readout>,
    /// Wider intervals calibrated against the simulation teacher's batch uncertainty.
    pub conservative: Option<Readout>,
}
#[derive(Debug, Serialize)]
pub struct Readout {
    pub targets: [Target; 3],
    /// Present only when the 95% crossing resolves to one CPU level.
    pub profile: Option<Profile>,
}
#[derive(Debug, Serialize)]
pub struct Profile {
    pub anchor_level: u8,
    pub skill_multiplier: f64,
    pub axes: [Axis; 8],
}
#[derive(Debug, Serialize)]
pub struct Axis {
    pub axis: &'static str,
    pub cost_pp: f64,
    pub interval: Option<[f64; 2]>,
    pub status: AxisStatus,
}
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AxisStatus {
    Relevant,
    Reversed,
    Neutral,
    Unresolved,
    StructuralZero,
    InsufficientCalibration,
}

fn readout(
    means: &[[f64; 9]; 10],
    widths: &[[f64; 9]; 10],
    has_mines: bool,
    three_keys: bool,
) -> Readout {
    let bounds = |i: usize, j: usize| {
        let v = means[i][j];
        let shape = if j == 0 {
            0.4 + (100. - v).abs() / 20.
        } else {
            0.04 + v.abs() / 4.
        };
        let radius = if j == 8 && !has_mines {
            0.
        } else {
            shape * widths[i][j]
        };
        [
            v - radius,
            if j == 0 {
                (v + radius).min(100.)
            } else {
                v + radius
            },
        ]
    };
    let reversed = (1..10).any(|i| bounds(i, 0)[1] < bounds(i - 1, 0)[0] - 0.5);
    let targets = [90u8, 95, 98].map(|goal| {
        let mut target = Target {
            target_pct: goal,
            level: None,
            status: TargetStatus::Nonmonotone,
            bracket_levels: [None, None],
        };
        if reversed {
            return target;
        }
        let below = (0..10)
            .take_while(|&i| bounds(i, 0)[1] < f64::from(goal))
            .count() as u8;
        let meets = (0..10)
            .find(|&i| bounds(i, 0)[0] >= f64::from(goal))
            .map(|i| i as u8 + 1);
        target.bracket_levels = [(below > 0).then_some(below), meets];
        target.status = if meets == Some(1) {
            TargetStatus::AtOrBelowRange
        } else if below == 10 {
            TargetStatus::AboveRange
        } else if meets == Some(below + 1) {
            target.level = meets;
            TargetStatus::Qualified
        } else {
            TargetStatus::Unresolved
        };
        target
    });
    let profile = targets[1].level.map(|level| Profile {
        anchor_level: level,
        skill_multiplier: 0.75,
        axes: array::from_fn(|k| {
            let i = usize::from(level - 1);
            let [lo, hi] = bounds(i, k + 1);
            let status = if k == 2 && three_keys {
                AxisStatus::InsufficientCalibration
            } else if k == 7 && !has_mines {
                AxisStatus::StructuralZero
            } else if lo > 0.1 {
                AxisStatus::Relevant
            } else if hi < -0.1 {
                AxisStatus::Reversed
            } else if lo >= -0.1 && hi <= 0.1 {
                AxisStatus::Neutral
            } else {
                AxisStatus::Unresolved
            };
            Axis {
                axis: SKILL_NAMES[k],
                cost_pp: means[i][k + 1],
                interval: (status != AxisStatus::InsufficientCalibration).then_some([lo, hi]),
                status,
            }
        }),
    });
    Readout { targets, profile }
}

fn map_rating(chart: &features::Chart, means: &[[f64; 9]; 10]) -> f64 {
    let loss = means
        .iter()
        .zip(coefficients::REFERENCE)
        .map(|(r, e)| ((100. - r[0]) * (1. + chart.units / chart.heads) - (100. - e)).max(0.))
        .sum::<f64>()
        / 10.;
    let signal = loss + 2. * chart.heads / chart.duration;
    let knots = &coefficients::KNOTS;
    let i = knots.partition_point(|&(x, _)| x <= signal);
    if i == 0 {
        return knots[0].1;
    }
    if i == knots.len() {
        return knots[i - 1].1;
    }
    let (x0, y0) = knots[i - 1];
    let (x1, y1) = knots[i];
    y0 + (y1 - y0) * (signal - x0) / (x1 - x0)
}

/// Estimate an original dance-single chart without simulating plays.
/// `index` is its zero-based index in the imported song.
pub fn estimate(song: &Song, index: usize) -> Result<Estimate, String> {
    let chart = features::extract(song, index)?;
    let means = network::predict(&chart);
    let decimal = map_rating(&chart, &means);
    let three_keys = chart.rows.iter().any(|r| r.heads >= 3.);
    let calibrated = |widths| {
        chart
            .readouts_studied
            .then(|| readout(&means, widths, chart.mines > 0, three_keys))
    };
    Ok(Estimate {
        estimator: ESTIMATOR,
        scale: SCALE,
        chart_digest: song.chart_digest(index),
        within_scalar_domain: chart.scalar_studied,
        within_studied_domain: chart.readouts_studied,
        within_matched_profile_domain: chart.matched_profile_studied,
        decimal,
        level: decimal.round() as u8,
        expected: calibrated(&coefficients::EXPECTED),
        conservative: calibrated(&coefficients::TEACHER),
        means,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::formats::sm::{parse_sm, parse_ssc};

    #[test]
    fn scalar_matched_profile_and_all_level_qualification() {
        for (notes, bpm, last_note, scalar, matched, readouts) in [
            (900, 120, "1000", true, true, true),
            (900, 120, "M000", true, true, true),
            (901, 120, "1000", true, true, false),
            (901, 120, "M000", true, false, false),
            (1200, 120, "1000", true, true, false),
            (1200, 120, "M000", true, false, false),
            (1201, 120, "1000", false, false, false),
            (901, 60, "1000", false, false, false), // over 180 seconds
            (901, 120, "L000", false, false, false),
        ] {
            let mut rows = vec!["1000"; notes];
            rows[notes - 1] = last_note;
            rows.resize(notes.next_multiple_of(16), "0000");
            let measures: Vec<_> = rows.chunks(16).map(|rows| rows.join("\n")).collect();
            let song = parse_sm(&format!(
                "#BPMS:0={bpm};#NOTES:dance-single::Hard:1::{};",
                measures.join(",\n")
            ))
            .unwrap();
            let estimate = estimate(&song, 0).unwrap();
            assert_eq!(estimate.within_scalar_domain, scalar);
            assert_eq!(estimate.within_matched_profile_domain, matched);
            let json = serde_json::to_string(&estimate).unwrap();
            let stored = ddi_library::manifest::ChartEstimate::from_rating_json(&json).unwrap();
            assert_eq!(stored.profile.is_some(), matched);
            assert_eq!(stored.studied, matched);
            let mut legacy = serde_json::to_value(&estimate).unwrap();
            legacy
                .as_object_mut()
                .unwrap()
                .remove("within_matched_profile_domain");
            let old = ddi_library::manifest::ChartEstimate::from_rating_json(&legacy.to_string())
                .unwrap();
            assert_eq!(old.profile.is_some(), readouts);
            assert_eq!(estimate.within_studied_domain, readouts);
            assert_eq!(estimate.expected.is_some(), readouts);
            assert_eq!(estimate.conservative.is_some(), readouts);
        }
    }

    #[test]
    fn bundled_chart_matches_the_frozen_scalar_curve_and_radar() {
        let song = parse_ssc(include_str!(
            "../../../assets/songs/beatucada/Beatucada.ssc"
        ))
        .unwrap();
        let r = estimate(&song, 5).unwrap();
        assert_eq!(
            r.chart_digest.to_string(),
            "b94f32ea53af58eb5e7fdf0f2eee315d"
        );
        assert!(r.within_studied_domain);
        assert!((r.decimal - 10.49627296).abs() < 1e-5);
        assert_eq!(r.level, 10);
        assert!((r.means[4][0] - 77.35601044).abs() < 1e-5);
        let p = r.expected.unwrap().profile.unwrap();
        assert_eq!(p.anchor_level, 10);
        assert_eq!(p.axes[7].status, AxisStatus::StructuralZero);
        assert!((p.axes[6].cost_pp - 0.10523347).abs() < 1e-5);
        assert!(r.conservative.unwrap().profile.is_none());
        assert!(estimate(&song, 2).is_err()); // dance-double
        assert!(estimate(&song, usize::MAX).is_err());
    }

    #[test]
    fn short_chart_estimates_and_unqualified_spokes_stay_distinct() {
        let song =
            parse_sm("#BPMS:0=120;#NOTES:dance-single::Easy:1::1000\n0100\n0010\n000L;").unwrap();
        let r = estimate(&song, 0).unwrap();
        assert!(!r.within_studied_domain);
        assert!(!r.within_scalar_domain);
        assert!(r.expected.is_none() && r.conservative.is_none());
        assert!((r.decimal - 5.51752514).abs() < 1e-5);
        // A unique 95% crossing does not qualify Handstream on three-key rows.
        let mut means = [[0.; 9]; 10];
        for (i, row) in means.iter_mut().enumerate() {
            row[0] = 90. + i as f64;
        }
        let p = readout(&means, &[[0.; 9]; 10], false, true)
            .profile
            .unwrap();
        assert_eq!(p.axes[2].status, AxisStatus::InsufficientCalibration);
        assert!(p.axes[2].interval.is_none());
    }
}
