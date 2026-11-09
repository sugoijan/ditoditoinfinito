// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Deterministic single-threaded whole-chart contacts. No I/O, clocks or workers.
mod burst;
mod double_geometry;
mod geometry;
mod history;
mod policy;
mod replay;
mod rolling;
mod search;
mod window_replay;
pub use burst::{BurstFallback, TapReduction};
use ddi_chart::{Note, NoteKind, Song, TimingMap};
use ddi_engine::transform::{self, AtOnce, TransformOptions};
pub use policy::{PlannerJob, PlanningReport, Status};
pub use replay::{JudgeReport, ReplayReport, replay};
pub use rolling::{ContactPublication, RollingPlanner, RollingReport};
pub use search::{Options, Search, SearchReport};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Input {
    pub notes: Vec<InputNote>,
    pub original_notes: usize,
    pub effective_notes: Vec<Note>,
    pub timing: TimingMap,
    pub layout: String,
    /// Actual gameplay judge tables; legacy studies use the three standard presets.
    pub rules: Vec<(String, ddi_engine::rules::JudgeTable)>,
}
#[derive(Clone, Copy, Debug)]
pub struct InputNote {
    pub tick: i64,
    pub time: f64,
    pub lane: u8,
    /// Physical mat tile, resolved once from the chart layout and lane.
    pub panel: u8,
    pub judgeable: bool,
    pub end: Option<(i64, f64)>,
    pub supported: bool,
    pub mine: bool,
    pub shock: bool,
    pub lift: bool,
    pub roll: bool,
    pub roll_refresh: bool,
}
/// Canonical scored tiles in chart-lane order. DanOni 9A Oni defaults to 5;
/// use `MatLayout::targets` for its two physical choices, never two chart notes.
pub fn layout_panels(layout: &str) -> Result<&'static [u8], String> {
    if layout == "danoni-9A" {
        return Ok(&[4, 2, 8, 6, 5, 13, 11, 17, 15]);
    }
    ddi_dance_mat::MatLayout::from_id(layout)
        .filter(|mat| mat.pads() == 1 || double_mat(mat.id()))
        .and_then(ddi_dance_mat::MatLayout::fixed_panels)
        .ok_or_else(|| "Dancing hand supports dance-single, dance-solo, pump-single, techno-single8, danoni-5, danoni-7, danoni-9B, dance-double, pump-halfdouble, pump-double and danoni-9A charts".into())
}
/// Explicit two-pad layouts currently supported by contact planning/gameplay.
pub(crate) fn double_mat(layout: &str) -> bool {
    matches!(
        layout,
        "dance-double" | "pump-halfdouble" | "pump-double" | "danoni-9A"
    )
}
pub(crate) fn double_pump(layout: &str) -> bool {
    matches!(layout, "pump-halfdouble" | "pump-double")
}
/// Physical geometry shared by planner, poses and gameplay rendering.
pub fn layout_geometry(layout: &str) -> Result<ddi_dance_mat::Geometry, String> {
    layout_panels(layout)?;
    Ok(if double_mat(layout) {
        ddi_dance_mat::Geometry::new(2, 24., 0.).expect("fixed double geometry")
    } else {
        ddi_dance_mat::Geometry::single()
    })
}
/// Whether the center is a scored lane, so it cannot be a safe escape tile.
pub(crate) fn scored_center(layout: &str) -> bool {
    layout_panels(layout).is_ok_and(|panels| panels.contains(&5))
}
/// Unscored support stances. An empty list requires air-time avoidance.
pub(crate) fn escape_stances(layout: &str) -> &'static [[u8; 2]] {
    match layout {
        "danoni-9A" => &[
            [7, 9],
            [9, 7],
            [9, 16],
            [16, 9],
            [16, 18],
            [18, 16],
            [1, 3],
            [3, 1],
            [3, 10],
            [10, 3],
            [10, 12],
            [12, 10],
        ],
        "dance-double" => &[[5, 5], [14, 14]],
        "pump-halfdouble" | "pump-double" => {
            &[[4, 6], [6, 4], [6, 13], [13, 6], [13, 15], [15, 13]]
        }
        "pump-single" => &[[4, 6], [6, 4]],
        "danoni-7" => &[[7, 3], [3, 7]],
        "dance-single" | "dance-solo" | "techno-single8" => &[[5, 5]],
        _ => &[],
    }
}
pub(crate) fn lift_escape(layout: &str, source: u8, foot: usize) -> u8 {
    if layout == "danoni-9A" {
        9 * ((source - 1) / 9) + if foot == 0 { 7 } else { 3 }
    } else if double_pump(layout) {
        9 * ((source - 1) / 9) + if foot == 0 { 4 } else { 6 }
    } else if layout == "dance-double" {
        5 + 9 * ((source - 1) / 9)
    } else {
        escape_stances(layout).first().map_or(source, |p| p[foot])
    }
}
/// Layouts using the full opposite-corner reach and flexible facing.
pub(crate) fn wide_stances(layout: &str) -> bool {
    matches!(
        layout,
        "pump-single"
            | "danoni-7"
            | "techno-single8"
            | "danoni-9B"
            | "dance-double"
            | "pump-halfdouble"
            | "pump-double"
            | "danoni-9A"
    )
}
/// Unique unscored support destinations in stable order.
pub(crate) fn support_tiles(layout: &str) -> &'static [u8] {
    match layout {
        "danoni-9A" => return &[1, 3, 7, 9, 10, 12, 16, 18],
        "dance-double" => return &[1, 3, 5, 7, 9, 10, 12, 14, 16, 18],
        "pump-halfdouble" => return &[1, 2, 4, 6, 7, 8, 11, 12, 13, 15, 17, 18],
        "pump-double" => return &[2, 4, 6, 8, 11, 13, 15, 17],
        _ => {}
    }
    match escape_stances(layout).first() {
        Some(stance) if stance[0] == stance[1] => &stance[..1],
        Some(stance) => stance,
        None => &[],
    }
}
pub fn layout_lanes(layout: &str) -> Result<usize, String> {
    layout_panels(layout).map(<[u8]>::len)
}
pub fn standard_rules() -> Vec<(String, ddi_engine::rules::JudgeTable)> {
    use ddi_engine::rules::presets;
    [presets::sm5(), presets::ddr_a(), presets::itg()]
        .into_iter()
        .map(|r| (r.id, r.judge))
        .collect()
}
/// Prepare the hand's own copy after gameplay transforms have resolved lanes.
/// No Stretch is active on double-mat layouts; single-mat cuts are unchanged.
/// This does not enable contact/pose support for a newly mapped layout.
pub fn prepare_notes(notes: &[Note], layout: &ddi_chart::Layout) -> Vec<Note> {
    transform::apply(
        notes,
        layout,
        &TransformOptions {
            at_once: AtOnce::Two,
            no_stretch: true,
            ..Default::default()
        },
        0,
    )
    .notes
}

fn default_layout() -> String {
    "dance-single".into()
}
fn four(layout: &str) -> bool {
    layout == "dance-single"
}
impl Input {
    /// Prepare a hand copy, validated against its own effective chart. Source
    /// notes, gameplay's judged chart and global settings are never changed.
    pub fn from_song(song: &Song, index: usize) -> Result<Self, String> {
        let c = song.charts.get(index).ok_or("chart missing")?;
        let panels = layout_panels(&c.layout)?;
        let effective = prepare_notes(&c.notes, &ddi_chart::Layout::builtin(&c.layout).unwrap());
        let timing = c.timing(song).clone();
        let notes = effective
            .iter()
            .map(|n| InputNote {
                tick: n.tick.0,
                time: timing.seconds_at(n.tick),
                lane: n.lane,
                panel: panels.get(n.lane as usize).copied().unwrap_or(0),
                judgeable: timing.judgeable(n.tick),
                end: n.kind.hold_end().map(|end| (end.0, timing.seconds_at(end))),
                supported: matches!(
                    n.kind,
                    NoteKind::Tap
                        | NoteKind::HoldHead { .. }
                        | NoteKind::RollHead { .. }
                        | NoteKind::Mine
                        | NoteKind::Shock
                        | NoteKind::Lift
                ),
                mine: matches!(n.kind, NoteKind::Mine | NoteKind::Shock),
                shock: n.kind == NoteKind::Shock,
                lift: n.kind == NoteKind::Lift,
                roll: matches!(n.kind, NoteKind::RollHead { .. }),
                roll_refresh: false,
            })
            .collect();
        Ok(Self {
            notes,
            original_notes: c.notes.len(),
            effective_notes: effective,
            timing,
            layout: c.layout.clone(),
            rules: standard_rules(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Foot {
    L,
    R,
}
impl Foot {
    pub fn index(self) -> usize {
        usize::from(self == Self::R)
    }
    pub(crate) fn at(d: usize) -> Self {
        if d == 0 { Self::L } else { Self::R }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Swing {
    pub release: f64,
    pub land: f64,
    pub from: u8,
    pub to: u8,
    pub lift_m: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub time: f64,
    pub music_time: f64,
    pub tick: i64,
    pub panel: u8,
    pub foot: Foot,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lift: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hold {
    pub panel: u8,
    pub end_time: f64,
    pub end_tick: i64,
    pub start: f64,
    pub end: f64,
    pub foot: Foot,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub roll: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Auxiliary {
    pub time: f64,
    pub foot: Foot,
    pub panel: u8,
    pub kind: RestoreKind,
    pub end_tick: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum RestoreKind {
    #[serde(rename = "companion_hop")]
    CompanionHop,
    #[serde(rename = "support_step")]
    SupportStep,
    #[serde(rename = "hold_restore")]
    HoldRestore,
    #[serde(rename = "mine_avoid")]
    MineAvoid,
    #[serde(rename = "lift_plant")]
    LiftPlant,
    #[serde(rename = "lift_clear")]
    LiftClear,
    #[serde(rename = "roll_refresh")]
    RollRefresh,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gap {
    pub foot: Foot,
    pub panel: u8,
    pub release: f64,
    pub restore: f64,
    pub end_tick: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_foot: Option<Foot>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimingAdjustment {
    pub foot: Foot,
    pub previous: f64,
    pub release: f64,
    pub land: f64,
    pub minimum_planted: f64,
    pub minimum_swing: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tracks {
    #[serde(rename = "L")]
    pub left: Vec<Swing>,
    #[serde(rename = "R")]
    pub right: Vec<Swing>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Initial {
    #[serde(rename = "L")]
    pub left: u8,
    #[serde(rename = "R")]
    pub right: u8,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub tile_pitch_mm: f64,
    pub panel_anchor_mm: f64,
    pub center_offset_mm: f64,
    pub other_offset_mm: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContactPlan {
    #[serde(default = "default_layout", skip_serializing_if = "four")]
    pub layout: String,
    pub version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mat: Option<ddi_dance_mat::Geometry>,
    pub scope: String,
    pub initial: Initial,
    pub placement: Placement,
    pub tracks: Tracks,
    pub events: Vec<Event>,
    pub source_music_origin_seconds: f64,
    pub duration: f64,
    pub holds: Vec<Hold>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mines: Vec<ddi_hand_pose::Mine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roll_pulses: Vec<ddi_hand_pose::RollPulse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aux_contacts: Option<Vec<Auxiliary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_gaps: Option<Vec<Gap>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_adjustments: Option<Vec<TimingAdjustment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_early_window_seconds: Option<f64>,
}
impl ContactPlan {
    pub fn pose_plan(&self) -> Result<ddi_hand_pose::Plan, String> {
        crate::search::rolls::validate_pulses(self)?;
        ddi_hand_pose::Plan::load(&serde_json::to_vec(self).map_err(|e| e.to_string())?)
    }
}

/// Resolve a physical contact back to its one logical chart button.
pub(crate) fn note_has_panel(layout: &str, n: &InputNote, panel: u8) -> bool {
    replay::lane(layout, panel) == Some(n.lane)
}
pub(crate) fn same_button(layout: &str, a: u8, b: u8) -> bool {
    a == b || (layout == "danoni-9A" && [5, 14].contains(&a) && [5, 14].contains(&b))
}
/// Hazards constrain both physical centers without duplicating the judged note.
pub(crate) fn physical_hazards(
    layout: &str,
    notes: impl Iterator<Item = InputNote>,
) -> Vec<InputNote> {
    notes.flat_map(|n| note_choices(layout, n)).collect()
}
fn note_choices(layout: &str, n: InputNote) -> impl Iterator<Item = InputNote> {
    let alias = (layout == "danoni-9A" && n.lane == 4).then_some(InputNote { panel: 14, ..n });
    std::iter::once(n).chain(alias)
}
