//! Types for the first migration slice, not a promise of URL acquisition yet.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub type Metadata = Map<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[serde(rename = "5")]
    Five,
    #[serde(rename = "7")]
    Seven,
    /// Historical source/reference compatibility only; not a playable target.
    #[serde(rename = "7i")]
    SevenI,
    #[serde(rename = "9A")]
    NineA,
    #[serde(rename = "9B")]
    NineB,
}
impl Mode {
    /// Current game targets. Historical reference types can retain retired modes.
    pub fn is_playable_label(label: &str) -> bool {
        matches!(label, "5" | "7" | "9A" | "9B")
    }
    pub fn is_playable(self) -> bool {
        Self::is_playable_label(self.label())
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Five => "5",
            Self::Seven => "7",
            Self::SevenI => "7i",
            Self::NineA => "9A",
            Self::NineB => "9B",
        }
    }
    pub fn lanes(self) -> usize {
        match self {
            Self::Five => 5,
            Self::Seven | Self::SevenI => 7,
            Self::NineA | Self::NineB => 9,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    pub taps_seconds: Vec<f64>,
    pub holds_seconds: Vec<[f64; 2]>,
    #[serde(flatten)]
    pub source: Metadata,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpeedEvent {
    pub seconds: f64,
    pub multiplier: f64,
    #[serde(flatten)]
    pub source: Metadata,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chart {
    pub id: String,
    pub mode: Mode,
    pub lanes: Vec<Lane>,
    #[serde(default)]
    pub speed: Vec<SpeedEvent>,
    #[serde(default)]
    pub metadata: Metadata,
    /// Uninterpreted source fields, including explicit null ratings and effects.
    #[serde(flatten)]
    pub source: Metadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicalGrid {
    pub bpm: f64,
    pub beat_zero_seconds: f64,
    pub subdivisions_per_beat: u8,
    #[serde(default = "default_budget")]
    pub maximum_error_ms: f64,
    /// Preserve the estimator/reviewer diagnostics without treating them as facts.
    #[serde(flatten)]
    pub evidence: Metadata,
}
fn default_budget() -> f64 {
    9.0
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeMotion {
    pub base_arrow_heights_per_second: f64,
    #[serde(flatten)]
    pub evidence: Metadata,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TierPolicy {
    #[default]
    Stars,
    Edit,
}

/// A review can explicitly leave a rating unknown; null is not a zero-star rating.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RatingPolicy {
    #[default]
    Source,
    Reviewed {
        stars: Option<u16>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct PreparationOptions {
    pub grid: Option<MusicalGrid>,
    pub native_motion: Option<NativeMotion>,
    pub scroll_factor: f64,
    pub tier_policy: TierPolicy,
    pub tier_bounds: [u16; 4],
    pub rating: RatingPolicy,
    pub preserve_label: bool,
}
impl Default for PreparationOptions {
    fn default() -> Self {
        Self {
            grid: None,
            native_motion: None,
            scroll_factor: 1.0,
            tier_policy: TierPolicy::Stars,
            tier_bounds: [1, 3, 5, 7],
            rating: RatingPolicy::Source,
            preserve_label: false,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileRequest {
    pub schema: String,
    pub chart: Chart,
    /// Already merged defaults/reference/chart metadata, as in load_export.
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub options: PreparationOptions,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum Tier {
    Beginner,
    Easy,
    Medium,
    Hard,
    Challenge,
    Edit,
}
impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
            Self::Challenge => "Challenge",
            Self::Edit => "Edit",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointKind {
    Tap,
    HoldHead,
    HoldTail,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum QuantizationMethod {
    ExactSourceFrame,
    RhythmicSubdivision,
    FineTickFallback,
    LaneCollisionFineFallback,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub lane: usize,
    pub kind: EndpointKind,
    pub index: usize,
    pub source_seconds: f64,
    pub export_seconds: f64,
    pub shift_ms: f64,
    /// Signed, unpadded, 48 ticks per beat. SSC adds whole-beat pre-roll separately.
    pub tick: i64,
    pub method: QuantizationMethod,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantizedLane {
    pub taps: Vec<i64>,
    pub holds: Vec<[i64; 2]>,
    pub collision_safeguard: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollEvent {
    pub source_seconds: f64,
    pub export_seconds: f64,
    pub shift_ms: f64,
    pub tick: i64,
    pub multiplier: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedChart {
    pub schema: &'static str,
    pub id: String,
    pub mode: Mode,
    pub difficulty_stars: Option<u16>,
    pub difficulty_classification: Tier,
    pub timing_bpm: f64,
    pub offset_seconds: f64,
    pub padding_ticks: i64,
    pub ssc_base_ratio: f64,
    pub lanes: Vec<QuantizedLane>,
    pub endpoints: Vec<Endpoint>,
    pub scroll_events: Vec<ScrollEvent>,
    pub maximum_note_error_ms: f64,
    pub unrepresented_effects: std::collections::BTreeMap<String, usize>,
    /// One #NOTEDATA block; the song-level media/preview headers are not emitted.
    pub ssc: String,
}
