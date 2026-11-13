//! Stage placements and the points they award. A stage's result is recorded
//! from confirmed outcomes only; the rule that awarded it is kept with it,
//! so a later rule never rewrites an earlier stage.
//!
//! Named rules: `EightPlayerV1` is the working eight-player table (12, 9,
//! 7, 5, 4, 3, 2, 1; `docs/plans/multiplayer.md`, "Stage resets and match
//! results"). Fields of nine and more read that table stretched down the
//! field and biased toward the top (`TopBiased*V1`, the maintainer,
//! 2026-11-13), the bias a match rule ([`PointsBias`]). Fields under eight
//! have no decided rule: `ExperimentalLinearV0` stands in, clearly
//! provisional, until the design decides; `DevelopmentLinearV0` awards any
//! field of a development-only practice room. `ExperimentalNineV0`, what
//! nine earned before the stretched table, is retired: never awarded by
//! this build, still read from an authority one build behind.

use crate::ids::ParticipantId;
use crate::points::Points;
use serde::{Deserialize, Serialize};

/// How a match turns stage placements into points. Locked with the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AwardScheme {
    /// Placement points summed over stages, equal stage weights. Eight
    /// starters use `EightPlayerV1`; fewer use the EXPERIMENTAL
    /// `ExperimentalLinearV0`; nine to [`TopBiased::MAX_FIELD`] the
    /// stretched table at the match's [`PointsBias`]; more have no rule.
    PlacementPointsV1,
    /// DEVELOPMENT ONLY: `DevelopmentLinearV0` for any field, for a practice
    /// room with far more CPUs than the cap (`RoomPolicy::development`).
    DevelopmentV0,
}

impl AwardScheme {
    /// The rule for a stage with `field` starters (sit-outs are not part of
    /// the field: a PROVISIONAL population basis) under the match's points
    /// bias (which a field of eight or fewer never reads).
    pub fn rule_for(self, field: usize, bias: PointsBias) -> Result<AwardRule, NoAwardRule> {
        match (self, field) {
            (Self::PlacementPointsV1, 8) => Ok(AwardRule::EightPlayerV1),
            (Self::PlacementPointsV1, 1..=7) => Ok(AwardRule::ExperimentalLinearV0),
            (Self::PlacementPointsV1, TopBiased::MIN_FIELD..=TopBiased::MAX_FIELD) => {
                Ok(AwardRule::top_biased(bias))
            }
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

/// How strongly a field of nine and more favors its top places: a match
/// rule (`crate::MatchRules::points`, the maintainer, 2026-11-13: "a match
/// rule with Strong being the default"). The eight-player table is read at
/// position `7 × u^k` for the share `u` down the field, with `k` falling
/// from 1 at eight players toward the bias `b` as the field grows
/// ([`TopBiased`]): the smaller `b`, the more the top places earn and the
/// further the lower ones are stretched out. ADDITIVE on the wire: left
/// out at Strong and read as Strong where missing.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PointsBias {
    /// b = 0.4 (the default).
    #[default]
    Strong,
    /// b = 0.5.
    Medium,
    /// b = 0.6.
    Mild,
}

impl PointsBias {
    pub const ALL: [PointsBias; 3] = [Self::Strong, Self::Medium, Self::Mild];

    pub fn is_strong(&self) -> bool {
        *self == Self::Strong
    }

    /// Its place in [`PointsBias::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The bias `b`, in tenths (4, 5, 6).
    pub fn tenths(self) -> u8 {
        match self {
            Self::Strong => 4,
            Self::Medium => 5,
            Self::Mild => 6,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Strong => "strong",
            Self::Medium => "medium",
            Self::Mild => "mild",
        }
    }
}

/// The stretched, top-biased table for fields of nine to
/// [`TopBiased::MAX_FIELD`] (`top-biased-v1`; the maintainer, 2026-11-13).
/// Place `p` of `N` (from 1) is `u = (p − 1) ÷ (N − 1)` down the field; the
/// eight-player table is read at position `7 × u^k` with
/// `k = b + (1 − b) × 7 ÷ (N − 1)`, linearly between its entries, and
/// rounded to a whole number (halves up); first place stands alone (any
/// other place that would round to 12 gets 11). `u^k` is irrational, so the
/// awards are FIXED TABLES here (computed once in double precision, every
/// value at least 2·10⁻⁴ from a rounding boundary: checked by the tests
/// against the formula), never a platform `powf` in a room's decision: the
/// room runs natively and as wasm and must give the same integers on both.
pub struct TopBiased;

impl TopBiased {
    pub const MIN_FIELD: usize = 9;
    /// The largest field with a table: the competitor cap
    /// (`RoomPolicy::INITIAL_COMPETITOR_CAP`).
    pub const MAX_FIELD: usize = 73;

    /// The awards for each place of a `field`-starter stage, best first.
    pub fn awards(bias: PointsBias, field: usize) -> Option<Vec<u64>> {
        if !(Self::MIN_FIELD..=Self::MAX_FIELD).contains(&field) {
            return None;
        }
        let table = match bias {
            PointsBias::Strong => &STRONG,
            PointsBias::Medium => &MEDIUM,
            PointsBias::Mild => &MILD,
        };
        let counts = &table[field - Self::MIN_FIELD];
        Some(
            counts
                .iter()
                .zip((1..=12u64).rev())
                .flat_map(|(&n, v)| std::iter::repeat_n(v, usize::from(n)))
                .collect(),
        )
    }
}

/// One named, versioned award rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AwardRule {
    /// The working eight-player table, for exactly eight starters.
    EightPlayerV1,
    /// [`TopBiased`] at [`PointsBias::Strong`], for nine to 73 starters.
    TopBiasedStrongV1,
    /// [`TopBiased`] at [`PointsBias::Medium`].
    TopBiasedMediumV1,
    /// [`TopBiased`] at [`PointsBias::Mild`].
    TopBiasedMildV1,
    /// EXPERIMENTAL, provisional: for a field of `n` under eight, place `p`
    /// earns `n + 1 - p` (one point per starter placed below, plus one), so
    /// last earns 1 and first earns `n`. Chosen for simplicity; it is not a
    /// truncation of the eight-player table and not a decided design.
    ExperimentalLinearV0,
    /// RETIRED (2026-11-13, replaced by the top-biased tables): for exactly
    /// nine starters, the eight-player table for the first eight places and
    /// nothing for ninth. Never awarded by this build; still read, as an
    /// authority one build behind awards nine so.
    ExperimentalNineV0,
    /// DEVELOPMENT ONLY: `ExperimentalLinearV0`'s `n + 1 - p` for any field,
    /// for practice with far more CPUs than the cap.
    DevelopmentLinearV0,
    /// A rule a newer authority named that this build does not know: its
    /// stage's placements and points are read as given (they are explicit),
    /// only the rule's name is lost. Never awarded.
    #[serde(other)]
    Unrecognized,
}

impl AwardRule {
    pub const EIGHT_PLAYER_TABLE: [u64; 8] = [12, 9, 7, 5, 4, 3, 2, 1];

    pub fn id(self) -> &'static str {
        match self {
            Self::EightPlayerV1 => "eight-player-v1",
            Self::TopBiasedStrongV1 => "top-biased-strong-v1",
            Self::TopBiasedMediumV1 => "top-biased-medium-v1",
            Self::TopBiasedMildV1 => "top-biased-mild-v1",
            Self::ExperimentalLinearV0 => "experimental-linear-v0",
            Self::ExperimentalNineV0 => "experimental-nine-v0",
            Self::DevelopmentLinearV0 => "development-linear-v0",
            Self::Unrecognized => "unrecognized",
        }
    }

    /// The top-biased table at `bias`.
    pub fn top_biased(bias: PointsBias) -> AwardRule {
        match bias {
            PointsBias::Strong => Self::TopBiasedStrongV1,
            PointsBias::Medium => Self::TopBiasedMediumV1,
            PointsBias::Mild => Self::TopBiasedMildV1,
        }
    }

    /// Not a decided design: results under it say so. The eight-player
    /// table and its stretch for larger fields are the maintainer's; fields
    /// under eight (and the retired nine, and development's) are not.
    pub fn experimental(self) -> bool {
        !matches!(
            self,
            Self::EightPlayerV1
                | Self::TopBiasedStrongV1
                | Self::TopBiasedMediumV1
                | Self::TopBiasedMildV1
        )
    }

    /// The award for each occupied position of a `field`-starter stage, best
    /// first, or `None` where this rule does not apply.
    pub fn awards(self, field: usize) -> Option<Vec<u64>> {
        match self {
            Self::EightPlayerV1 => (field == 8).then(|| Self::EIGHT_PLAYER_TABLE.to_vec()),
            Self::TopBiasedStrongV1 => TopBiased::awards(PointsBias::Strong, field),
            Self::TopBiasedMediumV1 => TopBiased::awards(PointsBias::Medium, field),
            Self::TopBiasedMildV1 => TopBiased::awards(PointsBias::Mild, field),
            Self::ExperimentalLinearV0 => (1..8)
                .contains(&field)
                .then(|| (1..=field as u64).rev().collect()),
            Self::ExperimentalNineV0 => (field == 9).then(|| {
                let mut table = Self::EIGHT_PLAYER_TABLE.to_vec();
                table.push(0);
                table
            }),
            Self::DevelopmentLinearV0 => (field > 0).then(|| (1..=field as u64).rev().collect()),
            Self::Unrecognized => None,
        }
    }
}

/// The fields [`TopBiased`] has a table for.
const FIELDS: usize = TopBiased::MAX_FIELD - TopBiased::MIN_FIELD + 1;

// The top-biased tables: for each field from nine, how many places earn
// 12, 11, … 1 (best first). Written from the formula by the tests'
// generator (`top_biased_tables_follow_the_formula`), which checks them.
#[rustfmt::skip]
const STRONG: [[u8; 12]; FIELDS] = [
    [1, 0, 0, 1, 0, 1, 0, 1, 1, 2, 1, 1], // 9
    [1, 0, 0, 1, 0, 1, 1, 1, 1, 1, 2, 1], // 10
    [1, 0, 0, 1, 0, 1, 1, 1, 1, 2, 2, 1], // 11
    [1, 0, 0, 1, 0, 1, 1, 1, 2, 2, 2, 1], // 12
    [1, 0, 0, 1, 0, 1, 1, 1, 2, 2, 2, 2], // 13
    [1, 0, 0, 1, 0, 1, 1, 1, 3, 2, 2, 2], // 14
    [1, 0, 0, 1, 0, 1, 1, 2, 2, 2, 3, 2], // 15
    [1, 0, 0, 1, 0, 1, 1, 2, 2, 3, 3, 2], // 16
    [1, 0, 0, 1, 0, 1, 1, 2, 3, 3, 3, 2], // 17
    [1, 0, 0, 1, 0, 1, 2, 1, 3, 3, 4, 2], // 18
    [1, 0, 0, 1, 1, 0, 2, 2, 2, 4, 4, 2], // 19
    [1, 0, 0, 1, 1, 1, 1, 2, 3, 3, 4, 3], // 20
    [1, 0, 0, 1, 1, 1, 1, 2, 3, 4, 4, 3], // 21
    [1, 0, 0, 1, 1, 1, 1, 2, 4, 4, 4, 3], // 22
    [1, 0, 0, 1, 1, 1, 1, 2, 4, 4, 5, 3], // 23
    [1, 0, 0, 1, 1, 1, 1, 2, 4, 5, 5, 3], // 24
    [1, 0, 0, 1, 1, 1, 1, 3, 4, 4, 6, 3], // 25
    [1, 0, 0, 1, 1, 1, 1, 3, 4, 5, 5, 4], // 26
    [1, 0, 0, 1, 1, 1, 1, 3, 4, 5, 6, 4], // 27
    [1, 0, 0, 1, 1, 1, 2, 2, 5, 5, 6, 4], // 28
    [1, 0, 0, 1, 1, 1, 2, 2, 5, 6, 6, 4], // 29
    [1, 0, 0, 1, 1, 1, 2, 3, 4, 6, 7, 4], // 30
    [1, 0, 0, 1, 1, 1, 2, 3, 5, 6, 7, 4], // 31
    [1, 0, 0, 1, 1, 1, 2, 3, 5, 6, 7, 5], // 32
    [1, 0, 0, 1, 1, 1, 2, 3, 5, 7, 7, 5], // 33
    [1, 0, 0, 1, 1, 1, 2, 3, 6, 6, 8, 5], // 34
    [1, 0, 0, 1, 1, 1, 2, 4, 5, 7, 8, 5], // 35
    [1, 0, 0, 1, 1, 1, 2, 4, 5, 8, 8, 5], // 36
    [1, 0, 0, 1, 1, 2, 1, 4, 6, 7, 9, 5], // 37
    [1, 0, 0, 1, 1, 2, 1, 4, 6, 8, 9, 5], // 38
    [1, 0, 0, 1, 1, 2, 2, 3, 6, 8, 9, 6], // 39
    [1, 0, 0, 1, 1, 2, 2, 3, 7, 8, 9, 6], // 40
    [1, 0, 0, 1, 1, 2, 2, 4, 6, 8, 10, 6], // 41
    [1, 0, 0, 1, 1, 2, 2, 4, 7, 8, 10, 6], // 42
    [1, 0, 0, 1, 1, 2, 2, 4, 7, 8, 11, 6], // 43
    [1, 0, 0, 1, 1, 2, 2, 4, 7, 9, 11, 6], // 44
    [1, 0, 0, 1, 1, 2, 2, 4, 8, 9, 10, 7], // 45
    [1, 0, 0, 1, 1, 2, 2, 5, 7, 9, 11, 7], // 46
    [1, 0, 0, 1, 1, 2, 2, 5, 7, 10, 11, 7], // 47
    [1, 0, 0, 1, 1, 2, 2, 5, 8, 9, 12, 7], // 48
    [1, 0, 0, 1, 1, 2, 3, 4, 8, 10, 12, 7], // 49
    [1, 0, 0, 1, 1, 2, 3, 4, 8, 10, 13, 7], // 50
    [1, 0, 0, 1, 1, 2, 3, 4, 9, 10, 12, 8], // 51
    [1, 0, 0, 1, 1, 2, 3, 5, 8, 10, 13, 8], // 52
    [1, 0, 0, 1, 1, 2, 3, 5, 8, 11, 13, 8], // 53
    [1, 0, 0, 1, 1, 2, 3, 5, 9, 11, 13, 8], // 54
    [1, 0, 0, 1, 1, 3, 2, 5, 9, 11, 14, 8], // 55
    [1, 0, 0, 1, 1, 3, 2, 5, 9, 12, 14, 8], // 56
    [1, 0, 0, 1, 2, 2, 2, 6, 9, 11, 14, 9], // 57
    [1, 0, 0, 1, 2, 2, 2, 6, 9, 12, 14, 9], // 58
    [1, 0, 0, 1, 2, 2, 3, 5, 9, 12, 15, 9], // 59
    [1, 0, 0, 1, 2, 2, 3, 5, 10, 12, 15, 9], // 60
    [1, 0, 0, 1, 2, 2, 3, 5, 10, 12, 16, 9], // 61
    [1, 0, 0, 1, 2, 2, 3, 5, 10, 13, 16, 9], // 62
    [1, 0, 0, 1, 2, 2, 3, 6, 10, 13, 15, 10], // 63
    [1, 0, 0, 1, 2, 2, 3, 6, 10, 13, 16, 10], // 64
    [1, 0, 0, 1, 2, 2, 3, 6, 10, 14, 16, 10], // 65
    [1, 0, 0, 1, 2, 2, 3, 6, 11, 13, 17, 10], // 66
    [1, 0, 0, 1, 2, 2, 3, 6, 11, 14, 17, 10], // 67
    [1, 0, 0, 1, 2, 2, 3, 6, 11, 14, 18, 10], // 68
    [1, 0, 0, 1, 2, 2, 3, 7, 11, 14, 17, 11], // 69
    [1, 0, 0, 1, 2, 2, 4, 6, 11, 14, 18, 11], // 70
    [1, 0, 0, 1, 2, 2, 4, 6, 11, 15, 18, 11], // 71
    [1, 0, 0, 1, 2, 2, 4, 6, 12, 14, 19, 11], // 72
    [1, 0, 0, 1, 2, 3, 3, 6, 12, 15, 19, 11], // 73
];

#[rustfmt::skip]
const MEDIUM: [[u8; 12]; FIELDS] = [
    [1, 0, 0, 1, 0, 1, 0, 1, 1, 2, 1, 1], // 9
    [1, 0, 0, 1, 0, 1, 1, 1, 1, 1, 2, 1], // 10
    [1, 0, 0, 1, 0, 1, 1, 1, 1, 2, 2, 1], // 11
    [1, 0, 0, 1, 1, 0, 1, 1, 2, 2, 2, 1], // 12
    [1, 0, 0, 1, 1, 0, 1, 1, 2, 2, 2, 2], // 13
    [1, 0, 0, 1, 1, 0, 1, 2, 2, 2, 2, 2], // 14
    [1, 0, 0, 1, 1, 1, 1, 1, 2, 3, 2, 2], // 15
    [1, 0, 0, 1, 1, 1, 1, 1, 3, 2, 3, 2], // 16
    [1, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 2], // 17
    [1, 0, 0, 1, 1, 1, 1, 2, 3, 3, 3, 2], // 18
    [1, 0, 0, 1, 1, 1, 1, 2, 3, 3, 4, 2], // 19
    [1, 0, 0, 1, 1, 1, 1, 2, 3, 4, 4, 2], // 20
    [1, 0, 0, 1, 1, 1, 2, 2, 3, 3, 4, 3], // 21
    [1, 0, 0, 1, 1, 1, 2, 2, 3, 4, 4, 3], // 22
    [1, 0, 0, 1, 1, 1, 2, 2, 4, 4, 4, 3], // 23
    [1, 0, 0, 1, 1, 2, 1, 2, 4, 4, 5, 3], // 24
    [1, 0, 0, 1, 1, 2, 1, 3, 4, 4, 5, 3], // 25
    [1, 0, 0, 1, 1, 2, 1, 3, 4, 5, 5, 3], // 26
    [1, 0, 0, 1, 1, 2, 1, 3, 4, 5, 6, 3], // 27
    [1, 0, 0, 1, 1, 2, 2, 2, 5, 5, 6, 3], // 28
    [1, 0, 0, 1, 2, 1, 2, 3, 4, 6, 5, 4], // 29
    [1, 0, 0, 1, 2, 1, 2, 3, 5, 5, 6, 4], // 30
    [1, 0, 0, 1, 2, 1, 2, 3, 5, 6, 6, 4], // 31
    [1, 0, 0, 1, 2, 1, 2, 4, 5, 5, 7, 4], // 32
    [1, 0, 0, 1, 2, 1, 2, 4, 5, 6, 7, 4], // 33
    [1, 0, 0, 1, 2, 2, 2, 3, 5, 7, 7, 4], // 34
    [1, 0, 0, 1, 2, 2, 2, 3, 6, 6, 8, 4], // 35
    [1, 0, 1, 0, 2, 2, 2, 4, 5, 7, 7, 5], // 36
    [1, 0, 1, 1, 1, 2, 2, 4, 6, 7, 7, 5], // 37
    [1, 0, 1, 1, 1, 2, 2, 4, 6, 7, 8, 5], // 38
    [1, 0, 1, 1, 1, 2, 2, 4, 7, 7, 8, 5], // 39
    [1, 0, 1, 1, 1, 2, 2, 5, 6, 7, 9, 5], // 40
    [1, 0, 1, 1, 1, 2, 3, 4, 6, 8, 9, 5], // 41
    [1, 0, 1, 1, 1, 2, 3, 4, 7, 8, 9, 5], // 42
    [1, 0, 1, 1, 1, 3, 2, 4, 7, 8, 9, 6], // 43
    [1, 0, 1, 1, 1, 3, 2, 5, 7, 8, 9, 6], // 44
    [1, 0, 1, 1, 2, 2, 2, 5, 7, 9, 9, 6], // 45
    [1, 0, 1, 1, 2, 2, 2, 5, 7, 9, 10, 6], // 46
    [1, 0, 1, 1, 2, 2, 3, 4, 8, 9, 10, 6], // 47
    [1, 0, 1, 1, 2, 2, 3, 5, 7, 9, 11, 6], // 48
    [1, 0, 1, 1, 2, 2, 3, 5, 8, 9, 11, 6], // 49
    [1, 0, 1, 1, 2, 2, 3, 5, 8, 10, 11, 6], // 50
    [1, 0, 1, 1, 2, 2, 3, 5, 9, 9, 11, 7], // 51
    [1, 0, 1, 1, 2, 2, 3, 6, 8, 10, 11, 7], // 52
    [1, 0, 1, 1, 2, 3, 3, 5, 8, 10, 12, 7], // 53
    [1, 0, 1, 1, 2, 3, 3, 5, 9, 10, 12, 7], // 54
    [1, 0, 1, 1, 2, 3, 3, 5, 9, 11, 12, 7], // 55
    [1, 0, 1, 1, 2, 3, 3, 6, 9, 10, 13, 7], // 56
    [1, 0, 1, 1, 2, 3, 3, 6, 9, 11, 13, 7], // 57
    [1, 0, 1, 1, 2, 3, 3, 6, 9, 12, 12, 8], // 58
    [1, 0, 1, 1, 2, 3, 3, 6, 10, 11, 13, 8], // 59
    [1, 0, 1, 1, 2, 3, 4, 6, 9, 12, 13, 8], // 60
    [1, 0, 1, 1, 3, 2, 4, 6, 10, 11, 14, 8], // 61
    [1, 0, 1, 1, 3, 2, 4, 6, 10, 12, 14, 8], // 62
    [1, 0, 1, 1, 3, 3, 3, 6, 11, 12, 14, 8], // 63
    [1, 0, 1, 1, 3, 3, 3, 7, 10, 12, 15, 8], // 64
    [1, 0, 1, 1, 3, 3, 3, 7, 10, 13, 14, 9], // 65
    [1, 0, 1, 1, 3, 3, 4, 6, 11, 13, 14, 9], // 66
    [1, 0, 1, 1, 3, 3, 4, 6, 11, 13, 15, 9], // 67
    [1, 0, 1, 1, 3, 3, 4, 7, 11, 13, 15, 9], // 68
    [1, 0, 1, 1, 3, 3, 4, 7, 11, 13, 16, 9], // 69
    [1, 0, 1, 2, 2, 3, 4, 7, 11, 14, 16, 9], // 70
    [1, 0, 1, 2, 2, 3, 4, 7, 12, 14, 16, 9], // 71
    [1, 0, 1, 2, 2, 3, 4, 8, 11, 14, 17, 9], // 72
    [1, 0, 1, 2, 2, 4, 4, 7, 12, 14, 16, 10], // 73
];

#[rustfmt::skip]
const MILD: [[u8; 12]; FIELDS] = [
    [1, 0, 0, 1, 0, 1, 0, 1, 2, 1, 1, 1], // 9
    [1, 0, 0, 1, 0, 1, 1, 1, 1, 1, 2, 1], // 10
    [1, 0, 0, 1, 1, 0, 1, 1, 2, 1, 2, 1], // 11
    [1, 0, 0, 1, 1, 0, 1, 1, 2, 2, 2, 1], // 12
    [1, 0, 0, 1, 1, 1, 0, 2, 2, 1, 2, 2], // 13
    [1, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2], // 14
    [1, 0, 0, 1, 1, 1, 1, 1, 3, 2, 2, 2], // 15
    [1, 0, 1, 0, 1, 1, 1, 2, 2, 3, 2, 2], // 16
    [1, 0, 1, 0, 1, 1, 1, 2, 3, 2, 3, 2], // 17
    [1, 0, 1, 0, 1, 1, 2, 1, 3, 3, 3, 2], // 18
    [1, 0, 1, 0, 1, 2, 1, 2, 3, 3, 3, 2], // 19
    [1, 0, 1, 0, 1, 2, 1, 2, 3, 3, 4, 2], // 20
    [1, 0, 1, 0, 2, 1, 1, 2, 4, 3, 4, 2], // 21
    [1, 0, 1, 1, 1, 1, 1, 3, 3, 4, 3, 3], // 22
    [1, 0, 1, 1, 1, 1, 2, 2, 3, 4, 4, 3], // 23
    [1, 0, 1, 1, 1, 1, 2, 2, 4, 4, 4, 3], // 24
    [1, 0, 1, 1, 1, 1, 2, 3, 3, 5, 4, 3], // 25
    [1, 0, 1, 1, 1, 2, 1, 3, 4, 4, 5, 3], // 26
    [1, 0, 1, 1, 1, 2, 1, 3, 4, 5, 5, 3], // 27
    [1, 0, 1, 1, 1, 2, 2, 3, 4, 5, 5, 3], // 28
    [1, 0, 1, 1, 1, 2, 2, 3, 4, 5, 6, 3], // 29
    [1, 0, 1, 1, 1, 2, 2, 3, 5, 5, 6, 3], // 30
    [1, 0, 1, 1, 2, 1, 2, 4, 4, 6, 5, 4], // 31
    [1, 0, 1, 1, 2, 1, 3, 3, 5, 5, 6, 4], // 32
    [1, 0, 1, 1, 2, 2, 2, 3, 5, 6, 6, 4], // 33
    [1, 0, 1, 1, 2, 2, 2, 3, 6, 6, 6, 4], // 34
    [1, 0, 1, 1, 2, 2, 2, 4, 5, 6, 7, 4], // 35
    [1, 0, 1, 1, 2, 2, 2, 4, 6, 6, 7, 4], // 36
    [1, 0, 1, 1, 2, 2, 3, 3, 6, 7, 7, 4], // 37
    [1, 0, 1, 1, 2, 2, 3, 4, 6, 6, 8, 4], // 38
    [1, 0, 1, 1, 2, 3, 2, 4, 6, 7, 8, 4], // 39
    [1, 0, 1, 2, 1, 3, 2, 4, 7, 7, 7, 5], // 40
    [1, 0, 1, 2, 2, 2, 2, 5, 6, 7, 8, 5], // 41
    [1, 0, 1, 2, 2, 2, 3, 4, 7, 7, 8, 5], // 42
    [1, 0, 1, 2, 2, 2, 3, 4, 7, 8, 8, 5], // 43
    [1, 0, 1, 2, 2, 2, 3, 5, 7, 7, 9, 5], // 44
    [1, 0, 1, 2, 2, 2, 3, 5, 7, 8, 9, 5], // 45
    [1, 0, 1, 2, 2, 3, 2, 5, 8, 8, 9, 5], // 46
    [1, 0, 1, 2, 2, 3, 3, 5, 7, 8, 10, 5], // 47
    [1, 0, 1, 2, 2, 3, 3, 5, 8, 8, 10, 5], // 48
    [1, 0, 1, 2, 2, 3, 3, 5, 8, 9, 9, 6], // 49
    [1, 0, 1, 2, 2, 3, 3, 6, 8, 8, 10, 6], // 50
    [1, 0, 1, 2, 3, 2, 4, 5, 8, 9, 10, 6], // 51
    [1, 0, 1, 2, 3, 3, 3, 5, 8, 10, 10, 6], // 52
    [1, 0, 2, 1, 3, 3, 3, 6, 8, 9, 11, 6], // 53
    [1, 0, 2, 1, 3, 3, 3, 6, 8, 10, 11, 6], // 54
    [1, 0, 2, 1, 3, 3, 3, 6, 9, 10, 11, 6], // 55
    [1, 0, 2, 1, 3, 3, 4, 5, 9, 10, 12, 6], // 56
    [1, 0, 2, 1, 3, 3, 4, 6, 9, 10, 11, 7], // 57
    [1, 0, 2, 2, 2, 3, 4, 6, 9, 11, 11, 7], // 58
    [1, 0, 2, 2, 2, 4, 3, 6, 10, 10, 12, 7], // 59
    [1, 1, 1, 2, 2, 4, 3, 7, 9, 11, 12, 7], // 60
    [1, 1, 1, 2, 3, 3, 4, 6, 10, 11, 12, 7], // 61
    [1, 1, 1, 2, 3, 3, 4, 6, 10, 11, 13, 7], // 62
    [1, 1, 1, 2, 3, 3, 4, 7, 10, 11, 13, 7], // 63
    [1, 1, 1, 2, 3, 3, 4, 7, 10, 12, 13, 7], // 64
    [1, 1, 1, 2, 3, 3, 4, 7, 11, 12, 13, 7], // 65
    [1, 1, 1, 2, 3, 4, 4, 7, 10, 12, 13, 8], // 66
    [1, 1, 1, 2, 3, 4, 4, 7, 11, 12, 13, 8], // 67
    [1, 1, 1, 2, 3, 4, 4, 7, 11, 13, 13, 8], // 68
    [1, 1, 1, 2, 3, 4, 4, 8, 11, 12, 14, 8], // 69
    [1, 1, 1, 2, 3, 4, 5, 7, 11, 13, 14, 8], // 70
    [1, 1, 1, 2, 4, 3, 5, 7, 12, 13, 14, 8], // 71
    [1, 1, 1, 2, 4, 4, 4, 8, 11, 13, 15, 8], // 72
    [1, 1, 1, 2, 4, 4, 4, 8, 12, 13, 15, 8], // 73
];

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
        let rule = scheme.rule_for(8, PointsBias::Strong).unwrap();
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
        let rule = AwardScheme::PlacementPointsV1.rule_for(4, PointsBias::Mild).unwrap();
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
        let s = PointsBias::Strong;
        assert_eq!(
            AwardScheme::PlacementPointsV1.rule_for(74, s),
            Err(NoAwardRule { field: 74 })
        );
        assert!(AwardScheme::PlacementPointsV1.rule_for(0, s).is_err());
        // Development practice awards any field, linearly.
        let dev = AwardScheme::DevelopmentV0.rule_for(129, s).unwrap();
        assert_eq!(dev, AwardRule::DevelopmentLinearV0);
        assert!(dev.experimental());
        assert_eq!(dev.awards(129).unwrap()[..2], [129, 128]);
    }

    /// A row as the brief writes it: "12 9 8×2 7×3".
    fn row(text: &str) -> Vec<u64> {
        text.split(' ')
            .flat_map(|part| {
                let (v, n) = part.split_once('×').unwrap_or((part, "1"));
                std::iter::repeat_n(v.parse::<u64>().unwrap(), n.parse().unwrap())
            })
            .collect()
    }

    #[test]
    fn the_top_biased_tables_give_the_maintainers_rows() {
        let table = |bias, n| {
            let rule = AwardScheme::PlacementPointsV1.rule_for(n, bias).unwrap();
            assert_eq!(rule, AwardRule::top_biased(bias));
            assert!(!rule.experimental());
            rule.awards(n).unwrap()
        };
        let strong = PointsBias::Strong;
        assert_eq!(table(strong, 9), row("12 9 7 5 4 3 3 2 1"));
        assert_eq!(table(strong, 12), row("12 9 7 6 5 4 4 3 3 2 2 1"));
        assert_eq!(table(strong, 16), row("12 9 7 6 5 5 4 4 3 3 3 2 2 2 1 1"));
        assert_eq!(table(strong, 33), row("12 9 8 7 6×2 5×3 4×5 3×7 2×7 1×5"));
        assert_eq!(
            table(strong, 73),
            row("12 9 8×2 7×3 6×3 5×6 4×12 3×15 2×19 1×11")
        );
        let medium = PointsBias::Medium;
        assert_eq!(table(medium, 9), row("12 9 7 5 4 3×2 2 1"));
        assert_eq!(table(medium, 16), row("12 9 8 7 6 5 4×3 3×2 2×3 1×2"));
        assert_eq!(table(medium, 33), row("12 9 8×2 7 6×2 5×4 4×5 3×6 2×7 1×4"));
        assert_eq!(
            table(medium, 73),
            row("12 10 9×2 8×2 7×4 6×4 5×7 4×12 3×14 2×16 1×10")
        );
        let mild = PointsBias::Mild;
        assert_eq!(table(mild, 9), row("12 9 7 5 4×2 3 2 1"));
        assert_eq!(table(mild, 16), row("12 10 8 7 6 5×2 4×2 3×3 2×2 1×2"));
        assert_eq!(
            table(mild, 33),
            row("12 10 9 8×2 7×2 6×2 5×3 4×5 3×6 2×6 1×4")
        );
        assert_eq!(
            table(mild, 73),
            row("12 11 10 9×2 8×4 7×4 6×4 5×8 4×12 3×13 2×15 1×8")
        );
        // Eight is the table itself, whatever the bias.
        for bias in PointsBias::ALL {
            let rule = AwardScheme::PlacementPointsV1.rule_for(8, bias).unwrap();
            assert_eq!(rule.awards(8).unwrap(), AwardRule::EIGHT_PLAYER_TABLE);
        }
    }

    #[test]
    fn every_field_from_nine_starts_at_twelve_alone_falls_and_ends_at_one() {
        for bias in PointsBias::ALL {
            for n in TopBiased::MIN_FIELD..=TopBiased::MAX_FIELD {
                let a = TopBiased::awards(bias, n).unwrap();
                assert_eq!(a.len(), n);
                assert_eq!(a[0], 12, "{bias:?} {n}");
                assert!(a[1] <= 11, "first alone: {bias:?} {n}");
                assert_eq!(a[n - 1], 1, "{bias:?} {n}");
                assert!(a.windows(2).all(|w| w[0] >= w[1]), "{bias:?} {n}");
            }
        }
    }

    /// The generator the tables were written from: the formula in double
    /// precision, each value's distance from a rounding boundary checked
    /// so that any correctly rounded `powf` gives the same integers.
    #[test]
    fn top_biased_tables_follow_the_formula() {
        let t = AwardRule::EIGHT_PLAYER_TABLE.map(|v| v as f64);
        let mut closest = f64::MAX;
        for bias in PointsBias::ALL {
            let b = f64::from(bias.tenths()) / 10.0;
            for n in TopBiased::MIN_FIELD..=TopBiased::MAX_FIELD {
                let last = (n - 1) as f64;
                let k = b + (1.0 - b) * 7.0 / last;
                let formula: Vec<u64> = (0..n)
                    .map(|p| {
                        let x = 7.0 * (p as f64 / last).powf(k);
                        let i = (x.floor() as usize).min(6);
                        let v = t[i] + (t[i + 1] - t[i]) * (x - i as f64);
                        if p > 0 && p < n - 1 {
                            closest = closest.min((v - v.floor() - 0.5).abs());
                        }
                        let r = (v + 0.5).floor() as u64;
                        if p > 0 { r.min(11) } else { r }
                    })
                    .collect();
                assert_eq!(TopBiased::awards(bias, n).unwrap(), formula, "{bias:?} {n}");
            }
        }
        assert!(closest > 1e-4, "a value is {closest} from a boundary");
    }

    #[test]
    fn ties_in_a_large_field_share_the_awards_they_occupy() {
        let rule = AwardRule::top_biased(PointsBias::Strong);
        // 33 starters: the first three tie (12 + 9 + 8), the rest single.
        let mut ranking = vec![group(&[1, 2, 3])];
        ranking.extend((4..=33).map(|i| group(&[i])));
        let result = StageResult::award(rule, &ranking).unwrap();
        let at = |i| result.placement(ParticipantId(i)).unwrap();
        assert_eq!(at(1).points, Points::ratio(29, 3));
        assert_eq!((at(2).place, at(3).place, at(4).place), (1, 1, 4));
        assert_eq!(at(4).points, Points::whole(7));
        assert_eq!(at(33).points, Points::whole(1));
        let total: Points = result.placements.iter().map(|p| p.points).sum();
        let table: u64 = TopBiased::awards(PointsBias::Strong, 33).unwrap().iter().sum();
        assert_eq!(total, Points::whole(table));
    }

    #[test]
    fn the_bias_is_additive_and_old_rules_are_still_read() {
        assert_eq!(PointsBias::default(), PointsBias::Strong);
        assert_eq!(PointsBias::ALL.map(PointsBias::index), [0, 1, 2]);
        // An authority one build behind awarded nine so; it is still read.
        let old: AwardRule = serde_json::from_str(r#""experimental_nine_v0""#).unwrap();
        assert_eq!(old, AwardRule::ExperimentalNineV0);
        assert!(old.experimental());
        assert_eq!(old.awards(9).unwrap()[8], 0);
        // ...and never chosen by this build.
        for bias in PointsBias::ALL {
            assert_ne!(
                AwardScheme::PlacementPointsV1.rule_for(9, bias),
                Ok(AwardRule::ExperimentalNineV0)
            );
        }
        // A newer authority's rule is read without breaking the result.
        let r: StageResult = serde_json::from_str(
            r#"{"rule":"someday_v9","placements":[{"participant":1,"place":1,"finish":"survived","points":{"num":3,"den":1}}]}"#,
        )
        .unwrap();
        assert_eq!(r.rule, AwardRule::Unrecognized);
        assert_eq!(r.placements[0].points, Points::whole(3));
        assert_eq!(
            serde_json::to_string(&AwardRule::TopBiasedMediumV1).unwrap(),
            r#""top_biased_medium_v1""#
        );
    }
}
