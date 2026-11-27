//! Prepared frozen-model laws and portable analytical CPU plans.
//! All preparation happens before gameplay; no inference or I/O in the input loop.
//! Sampling is deterministic from an explicit seed; the normal judge owns scores.

// SplitMix64, fixed algorithm on native and wasm; never global randomness.
use crate::transform::SplitMix64 as Random;
use crate::{Judge, JudgedNote, autoplay::Press, presets};
use ddi_chart::{ChartDigest, NoteKind, Song, Tick};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod absolute;
mod automatic;
mod basis;
/// Timing bucket boundaries in seconds for the absolute CPU motor.
pub const TIMING_EDGES: [f64; 89] = basis::EDGES;
mod full;
mod levels;
mod mines;
mod opponent;
mod personality;
pub use absolute::{ABSOLUTE_V1, ABSOLUTE_V2, Ability, AbsoluteModel, RowDemand, loads, respond as row_response};
pub use automatic::{Automatic, Plan};
pub use full::{CPU_PLAY, CpuPlay, FullPlan, HOLD_MODEL_V1, HoldModel, teacher_v1};
pub use levels::{
    AbilityTable, CPU_LEVELS, CPU_LEVELS_V0, CPU_LEVELS_V1, CPU_LEVELS_V2, LEVELS, Level, LevelScale,
    LevelTable,
    Strength,
};
pub use mines::{
    MINE_RESPONSE_V0, MINE_RESPONSE_V1, MineModel, MineResponse, MineResponseTable, ShockContext,
};
pub use opponent::{Opponent, PlannedRung, planned_charts, planned_ladder, planned_ladder_for};
pub use personality::{PATTERN_AXES, PRESET_NAMES, SKILL_NAMES, Skills};

pub const MAX_ROWS: usize = 4096;
/// Compact automatic plans do not keep five dense probability matrices.
pub const MAX_AUTOMATIC_ROWS: usize = 65_536;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The five frozen ability multipliers: the solo CPU's named strengths
/// ([`LABELS`]) and levels 6 to 10 of an opponent's ten
/// ([`CPU_LEVELS_V0`]).
pub const STRENGTHS: [f64; 5] = [0.75, 0.9, 1.0, 1.1, 1.25];
pub const LABELS: [&str; 5] = ["Relaxed", "Easy", "Normal", "Strong", "Expert"];
/// The chance any note is left out, whatever the level (the research
/// decoder's omitted component).
const OMITTED: f64 = 0.0001;

/// Check bounded, legal, chronological physical presses before a session starts.
pub fn validate_script(presses: &[Press]) -> Result<(), String> {
    validate(presses, false)
}

/// Check a full play's presses: bounded, legal, in time order, every key
/// held at least 12 ms, no two presses of one lane overlapping.
pub fn validate_full_script(presses: &[Press]) -> Result<(), String> {
    validate(presses, true)
}

/// Check a CPU's presses, whichever its play (a tap-only copy's keys,
/// each held 12 ms, pass the full play's test too).
pub fn validate_any_script(presses: &[Press]) -> Result<(), String> {
    validate(presses, true)
}

/// A tap-only copy's presses (`full` false: each key held exactly 12 ms)
/// or the chart's own (at least 12 ms, released within a day).
fn validate(presses: &[Press], full: bool) -> Result<(), String> {
    let most = if full { 64 } else { 4 };
    if presses.len() > MAX_AUTOMATIC_ROWS * most {
        return Err("CPU script exceeds the note limit".into());
    }
    let mut last = f64::NEG_INFINITY;
    let mut released = [f64::NEG_INFINITY; 4];
    for p in presses {
        let held = if full {
            p.release.abs() > 86401. || p.release - p.press < 0.012 - 1e-8
        } else {
            (p.release - p.press - 0.012).abs() > 1e-8
        };
        if p.lane >= 4
            || !p.press.is_finite()
            || !p.release.is_finite()
            || p.press.abs() > 86401.
            || held
            || p.press < last
            || p.press < released[p.lane as usize] + 0.0001 - 1e-9
        {
            return Err("Invalid or overlapping CPU key events".into());
        }
        last = p.press;
        released[p.lane as usize] = p.release;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub seconds: f64,
    pub mask: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    pub chart_digest: ChartDigest,
    /// Immutable research checkpoint/controller identity, not a quality label.
    pub model: String,
    pub rows: Vec<Row>,
    /// 89 edges for 88 uniform intervals, then atoms at -180/+180 ms.
    pub edges: Vec<f64>,
    /// Five strengths × rows × 91 masses (last: omitted component).
    pub laws: Vec<Vec<Vec<f64>>>,
}

/// Chart `index` of `song`, where a CPU can play its layout (one player,
/// four lanes).
fn check_layout(song: &Song, index: usize) -> Result<&ddi_chart::Chart, String> {
    let chart = song.charts.get(index).ok_or("CPU chart missing")?;
    if !song
        .layout_of(chart)
        .is_some_and(|layout| layout.players == 1 && layout.lanes.len() == 4)
    {
        return Err("CPU supports single-player keyboard 4K charts only".into());
    }
    Ok(chart)
}

/// Chart `index` of `song` judged as a CPU's plan reads it (its seconds
/// and judgeability; ITG's table, which neither depends on).
fn judge_for(song: &Song, index: usize) -> Judge {
    let chart = &song.charts[index];
    Judge::new(&chart.notes, chart.timing(song), presets::itg().judge)
}

/// A row of [`group_rows`]: its tick, the row and each lane's note.
type Grouped<K> = (Tick, Row, [Option<K>; 4]);

/// `notes`' judgeable ones grouped into rows by tick, in tick order: each
/// row's mask and second, and what `accept` made of each lane's note
/// (`Ok(None)`: not a row's; an error refuses the chart). Two notes of a
/// lane at one row refuse it.
fn group_rows<K: Copy>(
    notes: &[JudgedNote],
    mut accept: impl FnMut(&JudgedNote) -> Result<Option<K>, String>,
) -> Result<Vec<Grouped<K>>, String> {
    let mut by_tick: BTreeMap<Tick, (Row, [Option<K>; 4])> = BTreeMap::new();
    for n in notes.iter().filter(|n| n.judgeable) {
        let Some(kind) = accept(n)? else {
            continue;
        };
        let row = by_tick.entry(n.note.tick).or_insert((
            Row {
                seconds: n.seconds,
                mask: 0,
            },
            [None; 4],
        ));
        let bit = 1 << n.note.lane;
        if row.0.mask & bit != 0 {
            return Err("Duplicate CPU lane at one row".into());
        }
        row.0.mask |= bit;
        row.1[usize::from(n.note.lane)] = Some(kind);
    }
    Ok(by_tick.into_iter().map(|(t, (r, k))| (t, r, k)).collect())
}

/// Same note projection as the research pipeline; source song stays intact.
pub fn project(song: &Song, index: usize) -> Result<Song, String> {
    check_layout(song, index)?;
    let mut song = song.clone();
    song.charts[index].notes.retain_mut(|n| match n.kind {
        NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. } => {
            n.kind = NoteKind::Tap;
            true
        }
        _ => false,
    });
    Ok(song)
}

pub fn rows(notes: &[JudgedNote]) -> Result<Vec<Row>, String> {
    let rows: Vec<Row> = group_rows(notes, |n| {
        if n.note.kind != NoteKind::Tap || n.note.lane >= 4 || !n.seconds.is_finite() {
            return Err("CPU expects finite tap-only 4K notes".into());
        }
        Ok(Some(()))
    })?
    .into_iter()
    .map(|(_, r, _)| r)
    .collect();
    if !(32..=MAX_ROWS).contains(&rows.len())
        || rows.windows(2).any(|r| r[0].seconds >= r[1].seconds)
    {
        return Err(format!(
            "CPU needs 32–{MAX_ROWS} strictly increasing tap rows"
        ));
    }
    Ok(rows)
}

impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.model.is_empty()
            || self.model.len() > 256
            || !(32..=MAX_ROWS).contains(&self.rows.len())
            || self.edges.len() != 89
            || self.laws.len() != 5
        {
            return Err("Unsupported or oversized CPU profile".into());
        }
        if self.edges.iter().any(|v| !v.is_finite() || v.abs() > 0.18)
            || self.edges.windows(2).any(|x| x[0] >= x[1])
            || (self.edges[0] + 0.18).abs() > 1e-10
            || (self.edges[88] - 0.18).abs() > 1e-10
            || self.rows.iter().any(|r| {
                !r.seconds.is_finite() || r.seconds.abs() > 86400. || r.mask == 0 || r.mask > 15
            })
            || self.rows.windows(2).any(|r| r[0].seconds >= r[1].seconds)
        {
            return Err("Invalid CPU timing grid".into());
        }
        for level in &self.laws {
            if level.len() != self.rows.len() {
                return Err("CPU law row count differs".into());
            }
            for q in level {
                if q.len() != 91
                    || q.iter().any(|v| !v.is_finite() || *v < 0. || *v > 1.)
                    || (q.iter().sum::<f64>() - 1.).abs() > 1e-6
                    || q[90] >= 1.
                {
                    return Err("Invalid CPU probability law".into());
                }
            }
        }
        Ok(())
    }

    /// Validate source identity and projected times before any sampling.
    pub fn check_song(&self, song: &Song, index: usize) -> Result<Song, String> {
        self.validate()?;
        if index >= song.charts.len() || song.chart_digest(index) != self.chart_digest {
            return Err("CPU profile belongs to a different chart; prepare it again".into());
        }
        let projected = project(song, index)?;
        let actual = rows(judge_for(&projected, index).notes())?;
        if actual.len() != self.rows.len()
            || actual
                .iter()
                .zip(&self.rows)
                .any(|(a, b)| a.mask != b.mask || (a.seconds - b.seconds).abs() > 1e-8)
        {
            return Err("CPU profile timing differs from this chart".into());
        }
        Ok(projected)
    }

    /// Full-song motor state never resets at section boundaries. Three uniforms
    /// per note match the research decoder (interval, within interval, omission).
    pub fn script(&self, level: usize, seed: u64) -> Result<Vec<Press>, String> {
        self.validate()?;
        let laws = self.laws.get(level).ok_or("CPU strength must be 0..4")?;
        motor(&self.rows, seed, |i, u| {
            (u[2] >= OMITTED).then(|| self.offset(&laws[i], u[0], u[1]))
        })
    }

    fn offset(&self, q: &[f64], u: f64, within: f64) -> f64 {
        offset(&self.edges, q, u, within)
    }
}

/// Shared motor for both prepared and automatic laws. Keep draw order and
/// per-lane recovery independent of how the timing probabilities are obtained.
/// `sample` gets a row and a note's three uniforms (interval, within the
/// interval, omission) and gives its offset, or none where it is left out.
fn motor(
    rows: &[Row],
    seed: u64,
    mut sample: impl FnMut(usize, [f64; 3]) -> Option<f64>,
) -> Result<Vec<Press>, String> {
    let mut rng = Random(seed);
    let mut last_up = [f64::NEG_INFINITY; 4];
    let mut out = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        for (lane, last) in last_up.iter_mut().enumerate() {
            if row.mask & (1 << lane) == 0 {
                continue;
            }
            let u = [rng.unit(), rng.unit(), rng.unit()];
            let Some(offset) = sample(i, u) else {
                continue;
            };
            let press = (row.seconds + offset).max(*last + 0.0001);
            let release = press + 0.012;
            *last = release;
            out.push(Press {
                press,
                release,
                lane: lane as u8,
            });
        }
    }
    out.sort_by(|a, b| a.press.total_cmp(&b.press).then(a.lane.cmp(&b.lane)));
    validate_script(&out)?;
    Ok(out)
}

fn offset(edges: &[f64], q: &[f64], u: f64, within: f64) -> f64 {
    let threshold = u * (1. - q[90]);
    let mut total = 0.;
    let mut k = 89;
    for (i, p) in q[..90].iter().enumerate() {
        total += p;
        if threshold < total {
            k = i;
            break;
        }
    }
    match k {
        0..88 => edges[k] + (edges[k + 1] - edges[k]) * within,
        88 => -0.18,
        _ => 0.18,
    }
}


/// The five frozen ability multipliers, accepted by both launch interfaces.
pub fn strength(value: &str) -> Result<usize, String> {
    LABELS
        .iter()
        .position(|s| s.eq_ignore_ascii_case(value))
        .or_else(|| {
            value
                .parse::<f64>()
                .ok()
                .and_then(|x| STRENGTHS.iter().position(|s| (*s - x).abs() < 1e-9))
        })
        .ok_or_else(|| "CPU strength: relaxed, easy, normal, strong or expert".into())
}

/// The solo CPU's level where none is named (an index into the
/// opponents' scale, [`CPU_LEVELS`]: level 5 of 10, the middle),
/// since it plays the absolute scale by default (2026-11-14, the
/// maintainer: "The CPU CLI bots should move to the new levels").
pub const SOLO_LEVEL: usize = 4;

/// A named strength (an index into [`STRENGTHS`]) as a level of the
/// absolute scale: Relaxed to Expert are levels 6 to 10 (indexes 5 to 9),
/// as they were levels 6 to 10 of the chart-relative scale.
pub fn strength_level(strength: usize) -> usize {
    LEVELS - STRENGTHS.len() + strength.min(STRENGTHS.len() - 1)
}

/// A solo CPU's level on the opponents' absolute scale (`--cpu-level N`,
/// the web's `level=N`): `1` to `10`, as an index into [`CPU_LEVELS`]
/// (0 the weakest).
pub fn absolute_level(value: &str) -> Result<usize, String> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|l| (1..=LEVELS).contains(l))
        .map(|l| l - 1)
        .ok_or_else(|| format!("CPU level: 1 to {LEVELS}"))
}

/// The solo CPU's presses on `plan`: at its named strength (`strength`,
/// an index into [`STRENGTHS`], chart-relative, `cpu-levels-v0`, exactly
/// as before), or where `absolute` names one, at that level of the
/// opponents' absolute scale ([`CPU_LEVELS`], `cpu-levels-v2` since
/// round V; an automatic plan's alone).
pub fn solo_script(
    plan: &Plan,
    strength: usize,
    absolute: Option<usize>,
    seed: u64,
    skills: Skills,
) -> Result<Vec<Press>, String> {
    match absolute {
        None => plan.script_with_skills(strength, seed, skills),
        Some(level) => {
            let at = CPU_LEVELS
                .ability(level)
                .ok_or_else(|| format!("CPU level: 1 to {LEVELS}"))?;
            plan.script_play(
                Strength::Absolute(at),
                seed,
                skills,
                MINE_RESPONSE_V0.avoidance(level),
                solo_play(plan, absolute),
            )
        }
    }
}

/// What the solo CPU plays: at a level of the absolute scale on an
/// automatic plan, the chart itself ([`CPU_PLAY`]); at a named strength
/// or from a prepared profile, the tap-only copy, as before.
pub fn solo_play(plan: &Plan, absolute: Option<usize>) -> CpuPlay {
    match (plan, absolute) {
        (Plan::Automatic(_), Some(_)) => CPU_PLAY,
        _ => CpuPlay::TapsV0,
    }
}

/// A web play's explicit, non-sticky CPU selection. `1` prepares an analytical
/// plan from the loaded chart; other values are site-relative prepared JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub path: Option<String>,
    pub level: usize,
    /// A level of the opponents' absolute scale instead of the strength
    /// (`level=N`, [`absolute_level`]).
    pub absolute: Option<usize>,
    pub seed: u64,
    pub skills: Skills,
}
impl Selection {
    pub fn parse(profile: &str, level: &str, seed: &str) -> Result<Self, String> {
        let path = if profile == "1" {
            None
        } else {
            if profile.len() > 256
                || !profile.ends_with(".json")
                || profile
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
                || !profile
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/_.-".contains(&b))
            {
                return Err("CPU profile must be 1 or a site-relative JSON path".into());
            }
            Some(profile.into())
        };
        Ok(Self {
            path,
            absolute: None,
            skills: Skills::default(),
            level: strength(level)?,
            seed: seed
                .parse()
                .map_err(|_| "CPU seed must be an unsigned 64-bit integer")?,
        })
    }

    /// At level `value` (1 to 10) of the opponents' absolute scale
    /// instead of a strength; automatic mode alone.
    pub fn with_level(mut self, value: &str) -> Result<Self, String> {
        self.absolute = Some(absolute_level(value)?);
        if self.path.is_some() {
            return Err("A CPU level requires automatic mode (cpu=1)".into());
        }
        Ok(self)
    }

    /// Automatic mode's level where none was named: the named strength's
    /// ([`strength_level`]) where `named`, else [`SOLO_LEVEL`]. A prepared
    /// profile keeps its strength (chart-relative, as before).
    pub fn on_absolute_scale(mut self, named: bool) -> Self {
        if self.path.is_none() && self.absolute.is_none() {
            self.absolute = Some(if named {
                strength_level(self.level)
            } else {
                SOLO_LEVEL
            });
        }
        self
    }

    /// Its presses on `plan` ([`solo_script`]).
    pub fn script(&self, plan: &Plan) -> Result<Vec<Press>, String> {
        solo_script(plan, self.level, self.absolute, self.seed, self.skills)
    }

    /// What it plays of the chart ([`solo_play`]).
    pub fn play(&self, plan: &Plan) -> CpuPlay {
        solo_play(plan, self.absolute)
    }

    pub fn with_skills(mut self, value: &str) -> Result<Self, String> {
        self.skills = Skills::parse(value)?;
        if self.path.is_some() && self.skills != Skills::default() {
            return Err("CPU skill modifiers require automatic mode (cpu=1)".into());
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests;
