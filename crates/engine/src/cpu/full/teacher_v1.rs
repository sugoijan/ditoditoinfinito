//! Keyboard 4K difficulty under DDI Normal Practice. Estimate the first
//! absolute CPU level reaching each target mean raw Accuracy. At a qualified
//! 95% level, measure the Accuracy cost of weakening each skill in isolation.
//! Run offline or in a worker. Changes to the motor, judge or readout that
//! affect these ratings require a new teacher identity.
//!
//! Calculate JSON for a simfile's zero-based chart index:
//! `cargo run --release -p ddi-engine --example cpu_chart -- teacher-v1 FILE INDEX 128`.

use super::FullPlan;
use crate::autoplay;
use crate::cpu::{
    CPU_LEVELS_V2, MINE_RESPONSE_V0, MINE_RESPONSE_V1, PATTERN_AXES, SKILL_NAMES, Skills,
};
use crate::player::{PlayOptions, Player};
use crate::rules::ddi::{self, DdiOptions, Difficulty};
use ddi_chart::{ChartDigest, Song};
use serde::Serialize;

pub const ID: &str = "teacher-v1";
const SEEDS: [u64; 2] = [9_887_000_000_000, 9_887_001_000_000];
const WEAK: f64 = 0.75;
const DEADBAND: f64 = 0.10;

#[derive(Debug, Serialize)]
pub struct Rating {
    pub teacher: &'static str,
    pub chart_digest: ChartDigest,
    pub plays_per_batch: u32,
    /// 90%, 95% (primary) and 98% raw Accuracy, in that order.
    pub targets: [Target; 3],
    /// Available only at a qualified 95% level.
    pub profile: Option<Profile>,
    /// Two independent batches per balanced CPU level, ordered from 1 to 10.
    pub curve: Vec<[Estimate; 2]>,
    /// Lower level of each confirmed adjacent fall exceeding 0.5 percentage points.
    pub material_adjacent_reversals: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct Target {
    pub target_pct: u8,
    pub status: TargetStatus,
    pub level: Option<u8>,
    /// (Highest consecutively ruled-out level, first supported level].
    pub bracket_levels: [Option<u8>; 2],
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetStatus {
    Qualified,
    AtOrBelowRange,
    AboveRange,
    Unresolved,
    Nonmonotone,
}

#[derive(Debug, Serialize)]
pub struct Profile {
    pub anchor_level: u8,
    pub balanced_mean_pct: f64,
    pub skill_multiplier: f64,
    /// CPU skill order. Keep the status of each spoke when displaying its cost.
    pub axes: Vec<Axis>,
}

#[derive(Debug, Serialize)]
pub struct Axis {
    pub axis: &'static str,
    pub status: AxisStatus,
    /// Balanced minus weakened raw Accuracy in percentage points, using paired draws.
    pub cost_pp: f64,
    pub batches: [Estimate; 2],
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisStatus {
    Relevant,
    Reversed,
    Neutral,
    NotDetected,
    StructuralZero,
    Unresolved,
}

/// Monte Carlo interval for the mean (plus or minus three standard errors).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Estimate {
    pub mean: f64,
    pub lo: f64,
    pub hi: f64,
    pub se: f64,
}

type Batches = [Vec<f64>; 2];

fn estimates(batches: &Batches) -> [Estimate; 2] {
    batches.each_ref().map(|values| {
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let ss = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
        let se = (ss / (n * (n - 1.0))).sqrt();
        Estimate {
            mean,
            lo: mean - 3.0 * se,
            hi: mean + 3.0 * se,
            se,
        }
    })
}

fn difference(a: &Batches, b: &Batches) -> Batches {
    std::array::from_fn(|i| a[i].iter().zip(&b[i]).map(|(x, y)| x - y).collect())
}

fn target(curve: &[[Estimate; 2]], goal: u8, reversed: bool) -> Target {
    let g = f64::from(goal);
    let below = curve
        .iter()
        .take_while(|pair| pair.iter().all(|b| b.hi < g))
        .count() as u8;
    let upper = curve
        .iter()
        .position(|pair| pair.iter().all(|b| b.lo >= g))
        .map(|i| i as u8 + 1);
    let status = if reversed {
        TargetStatus::Nonmonotone
    } else if upper == Some(1) {
        TargetStatus::AtOrBelowRange
    } else if usize::from(below) == curve.len() {
        TargetStatus::AboveRange
    } else if upper == Some(below + 1) {
        TargetStatus::Qualified
    } else {
        TargetStatus::Unresolved
    };
    let level = upper.filter(|_| status == TargetStatus::Qualified);
    let bracket_levels = if reversed {
        [None, None]
    } else {
        [(below > 0).then_some(below), upper]
    };
    Target {
        target_pct: goal,
        status,
        level,
        bracket_levels,
    }
}

fn classify(costs: &Batches, batches: &[Estimate; 2]) -> AxisStatus {
    if costs.iter().flatten().all(|&d| d == 0.) {
        AxisStatus::NotDetected
    } else if batches.iter().all(|b| b.lo > DEADBAND) {
        AxisStatus::Relevant
    } else if batches.iter().all(|b| b.hi < -DEADBAND) {
        AxisStatus::Reversed
    } else if batches
        .iter()
        .all(|b| b.lo >= -DEADBAND && b.hi <= DEADBAND)
    {
        AxisStatus::Neutral
    } else {
        AxisStatus::Unresolved
    }
}

fn player(song: &Song, index: usize) -> Player {
    let rules = ddi::ruleset(&DdiOptions {
        difficulty: Difficulty::Normal,
        practice: true,
        boos: false,
    });
    Player::new(song, index, rules, PlayOptions::default())
}

struct Chart<'a> {
    song: &'a Song,
    index: usize,
    plan: FullPlan,
    end: f64,
    plays_per_batch: u32,
}

impl Chart<'_> {
    fn sample(&self, i: usize, skills: Skills) -> Result<Batches, String> {
        let ability = CPU_LEVELS_V2.levels[i];
        let mines = MINE_RESPONSE_V1
            .table
            .response(MINE_RESPONSE_V0.avoidance(i));
        let mut batches = [Vec::new(), Vec::new()];
        for (batch, first) in batches.iter_mut().zip(SEEDS) {
            for seed in first..first + u64::from(self.plays_per_batch) {
                let presses = super::script(&self.plan, ability, seed, skills, mines)?;
                let mut edges = autoplay::edges(presses);
                // The reference feeds key-downs before key-ups at identical times.
                edges.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.2.cmp(&a.2)));
                let mut p = player(self.song, self.index);
                p.start(0., 0.);
                let mut last = self.end;
                for (time, lane, down) in edges {
                    last = last.max(time);
                    p.input_at(lane, down, time);
                }
                p.update_at(last + 2.);
                if !p.judge().all_resolved() {
                    return Err("DDI judge could not finish the chart".into());
                }
                let ddi = p.results_brief().score.ddi.ok_or("Missing DDI score")?;
                batch.push(ddi.accuracy as f64 / 100.);
            }
        }
        Ok(batches)
    }

    fn profile(
        &self,
        level: u8,
        balanced: &Batches,
        at: &[Estimate; 2],
    ) -> Result<Profile, String> {
        let mut axes = Vec::with_capacity(SKILL_NAMES.len());
        let has_mines = self.plan.counts().2 > 0;
        for (index, axis) in SKILL_NAMES.into_iter().enumerate() {
            let (batches, status) = if index == PATTERN_AXES && !has_mines {
                let zero = Estimate {
                    mean: 0.,
                    lo: 0.,
                    hi: 0.,
                    se: 0.,
                };
                ([zero; 2], AxisStatus::StructuralZero)
            } else {
                let mut multipliers = [1.; SKILL_NAMES.len()];
                multipliers[index] = WEAK;
                let weak =
                    self.sample(usize::from(level - 1), Skills::from_values(&multipliers)?)?;
                let costs = difference(balanced, &weak);
                let batches = estimates(&costs);
                (batches, classify(&costs, &batches))
            };
            axes.push(Axis {
                axis,
                status,
                cost_pp: (batches[0].mean + batches[1].mean) / 2.,
                batches,
            });
        }
        Ok(Profile {
            anchor_level: level,
            balanced_mean_pct: (at[0].mean + at[1].mean) / 2.,
            skill_multiplier: WEAK,
            axes,
        })
    }
}

/// Rate one imported chart using 32–128 plays per batch. All conditions use
/// the same paired draws. Repeating a budget reproduces the rating.
pub fn rate(song: &Song, index: usize, plays_per_batch: u32) -> Result<Rating, String> {
    if !(32..=128).contains(&plays_per_batch) {
        return Err("teacher-v1 requires 32–128 plays per batch".into());
    }
    let chart = song
        .charts
        .get(index)
        .ok_or("Chart index is out of range")?;
    if chart.notes.len() > 8192 {
        return Err("teacher-v1 accepts up to 8,192 source notes".into());
    }
    let plan = FullPlan::new(song, index)?;
    if plan.rows.windows(2).any(|r| r[0].seconds >= r[1].seconds) {
        return Err("teacher-v1 requires strictly increasing grouped rows".into());
    }
    let initial = player(song, index);
    let end = initial
        .judge()
        .last_end_seconds()
        .ok_or("No judged notes")?;
    if initial
        .judge()
        .notes()
        .iter()
        .filter(|n| n.judgeable)
        .any(|n| {
            !n.seconds.is_finite()
                || !n.end_seconds.is_finite()
                || n.seconds < -60.
                || n.end_seconds > 1800.
        })
    {
        return Err("teacher-v1 requires judged times within −60 to 1,800 seconds".into());
    }
    let chart = Chart {
        song,
        index,
        plan,
        end,
        plays_per_batch,
    };
    let grid = (0..CPU_LEVELS_V2.levels.len())
        .map(|i| chart.sample(i, Skills::default()))
        .collect::<Result<Vec<_>, _>>()?;
    let curve: Vec<_> = grid.iter().map(estimates).collect();
    let material_adjacent_reversals: Vec<_> = grid
        .windows(2)
        .enumerate()
        .filter_map(|(i, pair)| {
            let change = estimates(&difference(&pair[1], &pair[0]));
            change.iter().all(|b| b.hi < -0.50).then_some(i as u8 + 1)
        })
        .collect();
    let targets = [90, 95, 98].map(|g| target(&curve, g, !material_adjacent_reversals.is_empty()));
    let profile = if let Some(level) = targets[1].level {
        let i = usize::from(level - 1);
        Some(chart.profile(level, &grid[i], &curve[i])?)
    } else {
        None
    };
    Ok(Rating {
        teacher: ID,
        chart_digest: song.chart_digest(index),
        plays_per_batch,
        targets,
        profile,
        curve,
        material_adjacent_reversals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::formats::sm::parse_sm;

    #[test]
    fn mixed_chart_matches_the_teacher_v1_reference() {
        let song = parse_sm("#BPMS:0=120;#NOTES:dance-single::Hard:5::\n1000\n0M00\n0200\n0010\n0300\n0004\n0100\n0003,\n000L\n0000\n1001\n0100;").unwrap();
        let rating = rate(&song, 0, 32).unwrap();
        assert_eq!(rating.targets[1].status, TargetStatus::Unresolved);
        assert_eq!(rating.targets[1].bracket_levels, [Some(7), None]);
        assert!(rating.profile.is_none());
        assert!((rating.curve[0][0].mean - 60.28).abs() < 1e-10);
        assert!((rating.curve[9][1].mean - 95.793125).abs() < 1e-10);
        assert!(rate(&song, 1, 32).is_err());
        assert!(rate(&song, 0, 31).is_err());
    }

    #[test]
    fn both_batches_must_support_the_first_level_and_each_spoke() {
        let batch = |v| estimates(&[vec![v; 32], vec![v; 32]]);
        let mut curve = [batch(90.), batch(94.), batch(96.)];
        assert_eq!(target(&curve, 95, false).level, Some(3));
        curve[1][1].hi = 95.1;
        let ambiguous = target(&curve, 95, false);
        assert_eq!(ambiguous.status, TargetStatus::Unresolved);
        assert_eq!(ambiguous.bracket_levels, [Some(1), Some(3)]);
        assert!(ambiguous.level.is_none());
        assert_eq!(target(&curve, 95, true).status, TargetStatus::Nonmonotone);
        assert_eq!(
            target(&curve, 90, false).status,
            TargetStatus::AtOrBelowRange
        );
        assert_eq!(target(&curve, 98, false).status, TargetStatus::AboveRange);
        for (a, b, status) in [
            (1., 1., AxisStatus::Relevant),
            (-1., -1., AxisStatus::Reversed),
            (1., 0., AxisStatus::Unresolved),
            (0., 0., AxisStatus::NotDetected),
            (0.02, 0., AxisStatus::Neutral),
        ] {
            let costs = [vec![a; 32], vec![b; 32]];
            assert_eq!(classify(&costs, &estimates(&costs)), status);
        }
    }
}
