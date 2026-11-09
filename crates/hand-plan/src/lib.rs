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
pub(crate) use ddi_hand_pose::wide_stances;
pub use ddi_hand_pose::{Event, Foot, Initial, Placement, Swing, Tracks};
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
impl InputNote {
    /// `n` on tile `panel`, timed by `timing`.
    pub fn new(n: &Note, panel: u8, timing: &TimingMap) -> Self {
        Self {
            tick: n.tick.0,
            time: timing.seconds_at(n.tick),
            lane: n.lane,
            panel,
            judgeable: timing.judgeable(n.tick),
            end: n.kind.hold_end().map(|end| (end.0, timing.seconds_at(end))),
            supported: n.kind.hand_supported(),
            mine: matches!(n.kind, NoteKind::Mine | NoteKind::Shock),
            shock: n.kind == NoteKind::Shock,
            lift: n.kind == NoteKind::Lift,
            roll: matches!(n.kind, NoteKind::RollHead { .. }),
            roll_refresh: false,
        }
    }
    /// A head pressed onto its tile: not a mine, a release or a roll's own
    /// refresh.
    pub(crate) fn is_head(&self) -> bool {
        !self.mine && !self.lift && !self.roll_refresh
    }
}
/// Canonical scored tiles in chart-lane order. DanOni 9A Oni defaults to 5;
/// use `MatLayout::targets` for its two physical choices, never two chart notes.
pub fn layout_panels(layout: &str) -> Result<&'static [u8], String> {
    ddi_dance_mat::MatLayout::from_id(layout)
        .filter(|mat| mat.pads() == 1 || double_mat(mat.id()))
        .map(ddi_dance_mat::MatLayout::panels)
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
/// Where a released finger lands, by stance on its source's pad.
fn local_lift_stance(layout: &str) -> Option<[u8; 2]> {
    match layout {
        "danoni-9A" => Some([7, 3]),
        "pump-halfdouble" | "pump-double" => Some([4, 6]),
        "dance-double" => Some([5, 5]),
        _ => escape_stances(layout).first().copied(),
    }
}
pub(crate) fn lift_escape(layout: &str, source: u8, foot: usize) -> u8 {
    local_lift_stance(layout).map_or(source, |stance| pad_offset(source) + stance[foot])
}
/// The id before a tile's pad's first: 0 on the left (or only) pad, 9 on the right.
pub(crate) fn pad_offset(tile: u8) -> u8 {
    9 * ((tile - 1) / 9)
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
#[cfg(test)]
#[test]
fn double_supports_are_the_unscored_tiles() {
    for layout in [
        "dance-double",
        "pump-halfdouble",
        "pump-double",
        "danoni-9A",
    ] {
        let mat = ddi_dance_mat::MatLayout::from_id(layout).unwrap();
        assert_eq!(
            support_tiles(layout),
            mat.unscored_tiles().map(|t| t.id()).collect::<Vec<_>>()
        );
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
            .map(|n| {
                let panel = panels.get(n.lane as usize).copied().unwrap_or(0);
                InputNote::new(n, panel, &timing)
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
        ddi_hand_pose::Plan::new(ddi_hand_pose::PlanParts {
            version: self.version,
            mat: self.mat,
            // The default layout goes unnamed, as in the plan's JSON.
            layout: if four(&self.layout) {
                String::new()
            } else {
                self.layout.clone()
            },
            duration: self.duration,
            initial: self.initial.clone(),
            tracks: self.tracks.clone(),
            placement: self.placement.clone(),
            events: self.events.clone(),
            mines: self.mines.clone(),
            roll_pulses: self.roll_pulses.clone(),
        })
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
