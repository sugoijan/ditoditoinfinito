//! Stage placements and the points they award. A stage's result is recorded
//! from confirmed outcomes only; the rule that awarded it is kept with it,
//! so a later rule never rewrites an earlier stage.
//!
//! Named rules: `EightPlayerV1` is the working eight-player table (12, 9,
//! 7, 5, 4, 3, 2, 1; `docs/plans/multiplayer.md`, "Stage resets and match
//! results"). Other fields have no decided rule: the table is never
//! silently truncated or rescaled for them. `ExperimentalLinearV0` stands
//! in under eight and `ExperimentalNineV0` for nine (the cap since M6b),
//! clearly provisional, until the design decides; `DevelopmentLinearV0`
//! awards any field of a development-only practice room.

use crate::ids::ParticipantId;
use crate::points::Points;
use serde::{Deserialize, Serialize};

/// How a match turns stage placements into points. Locked with the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AwardScheme {
    /// Placement points summed over stages, equal stage weights. Eight
    /// starters use `EightPlayerV1`; fewer use the EXPERIMENTAL
    /// `ExperimentalLinearV0`, nine the EXPERIMENTAL `ExperimentalNineV0`;
    /// more have no rule yet.
    PlacementPointsV1,
    /// DEVELOPMENT ONLY: `DevelopmentLinearV0` for any field, for a practice
    /// room with far more CPUs than the cap (`RoomPolicy::development`).
    DevelopmentV0,
}

impl AwardScheme {
    /// The rule for a stage with `field` starters (sit-outs are not part of
    /// the field: a PROVISIONAL population basis).
    pub fn rule_for(self, field: usize) -> Result<AwardRule, NoAwardRule> {
        match (self, field) {
            (Self::PlacementPointsV1, 8) => Ok(AwardRule::EightPlayerV1),
            (Self::PlacementPointsV1, 1..=7) => Ok(AwardRule::ExperimentalLinearV0),
            (Self::PlacementPointsV1, 9) => Ok(AwardRule::ExperimentalNineV0),
            (Self::DevelopmentV0, 1..) => Ok(AwardRule::DevelopmentLinearV0),
            _ => Err(NoAwardRule {
                field: field.try_into().unwrap_or(u16::MAX),
            }),
        }
    }
}

/// No rule awards points to a field of this size; the stage cannot be
/// confirmed with points until one is decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoAwardRule {
    pub field: u16,
}

/// One named, versioned award rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AwardRule {
    /// The working eight-player table, for exactly eight starters.
    EightPlayerV1,
    /// EXPERIMENTAL, provisional: for a field of `n` under eight, place `p`
    /// earns `n + 1 - p` (one point per starter placed below, plus one), so
    /// last earns 1 and first earns `n`. Chosen for simplicity; it is not a
    /// truncation of the eight-player table and not a decided design.
    ExperimentalLinearV0,
    /// EXPERIMENTAL, provisional: for exactly nine starters, the eight-player
    /// table for the first eight places and nothing for ninth (as a forfeit
    /// earns), so that the top eight earn what an eight-player field would.
    /// Not a decided design.
    ExperimentalNineV0,
    /// DEVELOPMENT ONLY: `ExperimentalLinearV0`'s `n + 1 - p` for any field,
    /// for practice with far more CPUs than the cap.
    DevelopmentLinearV0,
}

impl AwardRule {
    pub const EIGHT_PLAYER_TABLE: [u64; 8] = [12, 9, 7, 5, 4, 3, 2, 1];

    pub fn id(self) -> &'static str {
        match self {
            Self::EightPlayerV1 => "eight-player-v1",
            Self::ExperimentalLinearV0 => "experimental-linear-v0",
            Self::ExperimentalNineV0 => "experimental-nine-v0",
            Self::DevelopmentLinearV0 => "development-linear-v0",
        }
    }

    /// Not a decided design: results under it say so.
    pub fn experimental(self) -> bool {
        !matches!(self, Self::EightPlayerV1)
    }

    /// The award for each occupied position of a `field`-starter stage, best
    /// first, or `None` where this rule does not apply.
    pub fn awards(self, field: usize) -> Option<Vec<u64>> {
        match self {
            Self::EightPlayerV1 => (field == 8).then(|| Self::EIGHT_PLAYER_TABLE.to_vec()),
            Self::ExperimentalLinearV0 => (1..8)
                .contains(&field)
                .then(|| (1..=field as u64).rev().collect()),
            Self::ExperimentalNineV0 => (field == 9).then(|| {
                let mut table = Self::EIGHT_PLAYER_TABLE.to_vec();
                table.push(0);
                table
            }),
            Self::DevelopmentLinearV0 => (field > 0).then(|| (1..=field as u64).rev().collect()),
        }
    }
}

/// How a starter's stage ended, as the stage authority confirmed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    Survived,
    Eliminated,
    /// Left while competing, or a disconnect the rules turned into a forfeit.
    Forfeit,
}

/// One starter in a confirmed ranking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finisher {
    pub participant: ParticipantId,
    pub finish: Finish,
}

/// A starter's confirmed place and award.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub participant: ParticipantId,
    /// 1-based; tied starters share the best place of their group.
    pub place: u16,
    pub finish: Finish,
    pub points: Points,
}

/// A stage's confirmed result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageResult {
    pub rule: AwardRule,
    /// Best first.
    pub placements: Vec<Placement>,
}

impl StageResult {
    /// Award a confirmed ranking: groups best first, each group tied. Tied
    /// starters share the average of the awards their group occupies; a
    /// forfeit earns zero (PROVISIONAL).
    pub fn award(rule: AwardRule, ranking: &[Vec<Finisher>]) -> Option<Self> {
        let field = ranking.iter().map(Vec::len).sum();
        let awards = rule.awards(field)?;
        let mut placements = Vec::with_capacity(field);
        let mut at = 0;
        for group in ranking {
            let n = group.len();
            if n == 0 {
                continue;
            }
            let total: u64 = awards[at..at + n].iter().sum();
            let share = Points::ratio(total, n as u64);
            let place = u16::try_from(at + 1).unwrap_or(u16::MAX);
            placements.extend(group.iter().map(|f| Placement {
                participant: f.participant,
                place,
                finish: f.finish,
                // A confirmed forfeit earns zero (the connection-loss
                // study's proposal); its place's award goes to nobody.
                points: if f.finish == Finish::Forfeit {
                    Points::ZERO
                } else {
                    share
                },
            }));
            at += n;
        }
        Some(Self { rule, placements })
    }

    pub fn placement(&self, id: ParticipantId) -> Option<&Placement> {
        self.placements.iter().find(|p| p.participant == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(ids: &[u64]) -> Vec<Finisher> {
        ids.iter()
            .map(|&i| Finisher {
                participant: ParticipantId(i),
                finish: Finish::Survived,
            })
            .collect()
    }

    #[test]
    fn eight_starters_use_the_table_and_ties_average_occupied_awards() {
        let scheme = AwardScheme::PlacementPointsV1;
        let rule = scheme.rule_for(8).unwrap();
        assert_eq!(rule, AwardRule::EightPlayerV1);
        assert!(!rule.experimental());
        let ranking = vec![
            group(&[1, 2]),
            group(&[3]),
            group(&[4, 5, 6]),
            group(&[7]),
            group(&[8]),
        ];
        let result = StageResult::award(rule, &ranking).unwrap();
        let points = |i| result.placement(ParticipantId(i)).unwrap();
        assert_eq!(points(1).points, Points::ratio(21, 2));
        assert_eq!((points(1).place, points(2).place), (1, 1));
        assert_eq!(points(3).place, 3);
        assert_eq!(points(3).points, Points::whole(7));
        assert_eq!(points(4).points, Points::whole(4)); // (5 + 4 + 3) / 3
        assert_eq!(points(4).place, 4);
        assert_eq!(points(8).points, Points::whole(1));
        let total: Points = result.placements.iter().map(|p| p.points).sum();
        assert_eq!(total, Points::whole(43));
    }

    #[test]
    fn small_fields_use_the_named_experimental_rule_never_a_truncated_table() {
        let rule = AwardScheme::PlacementPointsV1.rule_for(4).unwrap();
        assert_eq!(rule, AwardRule::ExperimentalLinearV0);
        assert!(rule.experimental());
        assert_eq!(rule.id(), "experimental-linear-v0");
        assert_eq!(rule.awards(4), Some(vec![4, 3, 2, 1]));
        assert_eq!(rule.awards(8), None);
        assert_eq!(AwardRule::EightPlayerV1.awards(4), None);
        let result = StageResult::award(rule, &[group(&[1]), group(&[2, 3]), group(&[4])]).unwrap();
        assert_eq!(
            result.placement(ParticipantId(2)).unwrap().points,
            Points::ratio(5, 2)
        );
        assert_eq!(result.placement(ParticipantId(4)).unwrap().place, 4);
    }

    #[test]
    fn a_forfeit_earns_zero_and_leaves_its_award_unused() {
        let rule = AwardRule::ExperimentalLinearV0;
        let mut ranking = vec![group(&[1]), group(&[2])];
        ranking.push(vec![Finisher {
            participant: ParticipantId(3),
            finish: Finish::Forfeit,
        }]);
        let result = StageResult::award(rule, &ranking).unwrap();
        assert_eq!(
            result.placement(ParticipantId(1)).unwrap().points,
            Points::whole(3)
        );
        let forfeit = result.placement(ParticipantId(3)).unwrap();
        assert_eq!((forfeit.place, forfeit.points), (3, Points::ZERO));
    }

    #[test]
    fn fields_beyond_the_rules_have_no_award() {
        assert_eq!(
            AwardScheme::PlacementPointsV1.rule_for(10),
            Err(NoAwardRule { field: 10 })
        );
        assert!(AwardScheme::PlacementPointsV1.rule_for(0).is_err());
        // Development practice awards any field, linearly.
        let dev = AwardScheme::DevelopmentV0.rule_for(65).unwrap();
        assert_eq!(dev, AwardRule::DevelopmentLinearV0);
        assert!(dev.experimental());
        assert_eq!(dev.awards(65).unwrap()[..2], [65, 64]);
    }

    #[test]
    fn nine_take_the_eight_player_table_and_nothing_for_ninth() {
        let rule = AwardScheme::PlacementPointsV1.rule_for(9).unwrap();
        assert_eq!(rule, AwardRule::ExperimentalNineV0);
        assert!(rule.experimental());
        assert_eq!(rule.awards(9).unwrap(), vec![12, 9, 7, 5, 4, 3, 2, 1, 0]);
        assert_eq!(rule.awards(8), None);
        let ranking: Vec<Vec<Finisher>> = (1..=9)
            .map(|i| {
                vec![Finisher {
                    participant: ParticipantId(i),
                    finish: Finish::Survived,
                }]
            })
            .collect();
        let result = StageResult::award(rule, &ranking).unwrap();
        assert_eq!(
            result.placement(ParticipantId(1)).unwrap().points,
            Points::whole(12)
        );
        assert_eq!(
            result.placement(ParticipantId(9)).unwrap().points,
            Points::ZERO
        );
    }
}
