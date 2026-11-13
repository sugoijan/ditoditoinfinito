//! Prepared frozen-model laws and portable analytical CPU plans.
//! All preparation happens before gameplay; no inference or I/O in the input loop.
//! Sampling is deterministic from an explicit seed; the normal judge owns scores.

use crate::{Judge, JudgedNote, autoplay::Press, presets};
use ddi_chart::{ChartDigest, NoteKind, Song, Tick};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod automatic;
mod basis;
mod levels;
mod mines;
mod opponent;
mod personality;
pub use automatic::{Automatic, Plan};
pub use levels::{CPU_LEVELS_V0, LEVELS, Level, LevelTable};
pub use mines::{
    MINE_RESPONSE_V0, MINE_RESPONSE_V1, MineModel, MineResponse, MineResponseTable, ShockContext,
};
pub use opponent::{Opponent, PlannedRung, planned_ladder};
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
    if presses.len() > MAX_AUTOMATIC_ROWS * 4 {
        return Err("CPU script exceeds the note limit".into());
    }
    let mut last = f64::NEG_INFINITY;
    let mut released = [f64::NEG_INFINITY; 4];
    for p in presses {
        if p.lane >= 4
            || !p.press.is_finite()
            || !p.release.is_finite()
            || p.press.abs() > 86401.
            || (p.release - p.press - 0.012).abs() > 1e-8
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

/// Same note projection as the research pipeline; source song stays intact.
pub fn project(song: &Song, index: usize) -> Result<Song, String> {
    let chart = song.charts.get(index).ok_or("CPU chart missing")?;
    if !song
        .layout_of(chart)
        .is_some_and(|layout| layout.players == 1 && layout.lanes.len() == 4)
    {
        return Err("CPU supports single-player keyboard 4K charts only".into());
    }
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
    let mut rows: BTreeMap<Tick, Row> = BTreeMap::new();
    for n in notes.iter().filter(|n| n.judgeable) {
        if n.note.kind != NoteKind::Tap || n.note.lane >= 4 || !n.seconds.is_finite() {
            return Err("CPU expects finite tap-only 4K notes".into());
        }
        let r = rows.entry(n.note.tick).or_insert(Row {
            seconds: n.seconds,
            mask: 0,
        });
        let bit = 1 << n.note.lane;
        if r.mask & bit != 0 {
            return Err("Duplicate CPU lane at one row".into());
        }
        r.mask |= bit;
    }
    let rows: Vec<_> = rows.into_values().collect();
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
        let chart = &projected.charts[index];
        let judge = Judge::new(&chart.notes, chart.timing(&projected), presets::itg().judge);
        let actual = rows(judge.notes())?;
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

/// SplitMix64, fixed algorithm on native and wasm; never global randomness.
struct Random(u64);
impl Random {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^= z >> 31;
        (z >> 11) as f64 * (1. / ((1u64 << 53) as f64))
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

/// A web play's explicit, non-sticky CPU selection. `1` prepares an analytical
/// plan from the loaded chart; other values are site-relative prepared JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub path: Option<String>,
    pub level: usize,
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
            skills: Skills::default(),
            level: strength(level)?,
            seed: seed
                .parse()
                .map_err(|_| "CPU seed must be an unsigned 64-bit integer")?,
        })
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
