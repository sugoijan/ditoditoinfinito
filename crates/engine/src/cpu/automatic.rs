//! Portable analytical CPU v1. This is a new runtime policy, not an export of
//! the learned MSD-conditioned checkpoints. The frequency recipe, timing grid,
//! 9–80 ms width bounds and input motor come from the research baseline.
use super::absolute::{self, RowDemand};
use super::full::{self, CpuPlay, FullPlan};
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    Prepared(Profile),
    Automatic(Box<Automatic>),
}

impl Plan {
    pub fn automatic(song: &Song, index: usize) -> Result<Self, String> {
        Automatic::new(song, index).map(|p| Self::Automatic(Box::new(p)))
    }

    pub fn check_song(&self, song: &Song, index: usize) -> Result<Song, String> {
        match self {
            Self::Prepared(p) => p.check_song(song, index),
            Self::Automatic(p) => {
                if index >= song.charts.len() || song.chart_digest(index) != p.digest {
                    return Err("CPU plan belongs to a different chart".into());
                }
                let projected = project(song, index)?;
                if chart_rows(&projected, index)? != p.rows {
                    return Err("CPU plan timing differs from this chart".into());
                }
                Ok(projected)
            }
        }
    }

    pub fn script(&self, level: usize, seed: u64) -> Result<Vec<Press>, String> {
        self.script_with_skills(level, seed, Skills::default())
    }

    pub fn script_with_skills(
        &self,
        level: usize,
        seed: u64,
        skills: Skills,
    ) -> Result<Vec<Press>, String> {
        match self {
            Self::Prepared(p) if skills == Skills::default() => p.script(level, seed),
            Self::Prepared(_) => Err("CPU skill modifiers require an automatic plan".into()),
            Self::Automatic(p) if skills == Skills::default() => p.script(level, seed),
            Self::Automatic(p) => p.script_with_skills(level, seed, skills),
        }
    }

    /// The presses of a play at `level` (an opponent's: one of the ten,
    /// [`CPU_LEVELS_V0`], or a study's own): a frozen strength exactly as
    /// [`Plan::script_with_skills`] plans it, a weaker level by the
    /// analytical plan with its lapses (never a prepared profile, which
    /// has the five frozen laws alone).
    pub fn script_at(&self, level: Level, seed: u64, skills: Skills) -> Result<Vec<Press>, String> {
        if let Some(index) = level.frozen() {
            return self.script_with_skills(index, seed, skills);
        }
        level.validate()?;
        match self {
            Self::Prepared(_) => {
                Err("CPU levels beyond the five strengths require an automatic plan".into())
            }
            Self::Automatic(p) => p.script_at(level, seed, skills),
        }
    }

    /// The presses of a play at `strength`: a relative level as
    /// [`Plan::script_at`] plans it, an absolute one by its abilities
    /// (`super::absolute`; never a prepared profile).
    pub fn script_strength(
        &self,
        strength: Strength,
        seed: u64,
        skills: Skills,
    ) -> Result<Vec<Press>, String> {
        match strength {
            Strength::Relative(level) => self.script_at(level, seed, skills),
            Strength::Absolute(ability) => match self {
                Self::Prepared(_) => Err("CPU levels need an automatic plan".into()),
                Self::Automatic(p) => {
                    absolute::script(&p.rows, &p.absolute, ability, seed, skills)
                }
            },
        }
    }

    /// Chart `index` of `song` as a CPU playing `play` is given it: the
    /// tap-only copy ([`Plan::check_song`]) or the chart itself, checked
    /// to be this plan's (an automatic plan's alone).
    pub fn check_song_for(&self, song: &Song, index: usize, play: CpuPlay) -> Result<Song, String> {
        match play {
            CpuPlay::TapsV0 => self.check_song(song, index),
            CpuPlay::FullV1 => {
                let Self::Automatic(p) = self else {
                    return Err("The full chart needs an automatic CPU plan".into());
                };
                if index >= song.charts.len() || song.chart_digest(index) != p.digest {
                    return Err("CPU plan belongs to a different chart".into());
                }
                if p.full.is_none() {
                    return Err("CPU cannot play this chart whole".into());
                }
                Ok(song.clone())
            }
        }
    }

    /// The presses of a play at `strength` of `play`: the tap-only copy
    /// as [`Plan::script_strength`] plans it, or the chart itself
    /// (`cpu-play-v1`, an absolute level's alone), its mines met with
    /// the response of `avoidance` (`0..=1`, the level's,
    /// [`MINE_RESPONSE_V0`]`.avoidance`).
    pub fn script_play(
        &self,
        strength: Strength,
        seed: u64,
        skills: Skills,
        avoidance: f64,
        play: CpuPlay,
    ) -> Result<Vec<Press>, String> {
        match (play, strength) {
            (CpuPlay::TapsV0, _) => self.script_strength(strength, seed, skills),
            (CpuPlay::FullV1, Strength::Absolute(ability)) => match self {
                Self::Automatic(p) => {
                    let plan = p.full.as_deref().ok_or("CPU cannot play this chart whole")?;
                    full::script(
                        plan,
                        ability,
                        seed,
                        skills,
                        MINE_RESPONSE_V1.table.response(avoidance),
                    )
                }
                Self::Prepared(_) => Err("The full chart needs an automatic CPU plan".into()),
            },
            (CpuPlay::FullV1, Strength::Relative(_)) => {
                Err("The full chart is played at an absolute level".into())
            }
        }
    }

    /// The chart itself as a CPU plays it (an automatic plan's), for
    /// studies.
    pub fn full(&self) -> Option<&FullPlan> {
        match self {
            Self::Prepared(_) => None,
            Self::Automatic(p) => p.full.as_deref(),
        }
    }

    /// Each row's demand in real units (an automatic plan's; none for a
    /// prepared profile), for studies.
    pub fn demands(&self) -> &[RowDemand] {
        match self {
            Self::Prepared(_) => &[],
            Self::Automatic(p) => &p.absolute,
        }
    }

    pub fn model(&self) -> &str {
        match self {
            Self::Prepared(p) => &p.model,
            Self::Automatic(_) => "analytical-runtime-v1",
        }
    }
}

/// Rows, frequencies and seven causal demands per event, plus six small laws. No
/// per-chart model, Python, teacher, network call, or dense rows×strengths×bins.
#[derive(Clone, Debug, PartialEq)]
pub struct Automatic {
    digest: ChartDigest,
    rows: Vec<Row>,
    frequency: Vec<f64>,
    good: [f64; 91],
    bad: [[f64; 91]; 5],
    width: f64,
    demand: Vec<[f64; 7]>,
    /// Each row's demand in real units (`cpu-levels-v1`).
    absolute: Vec<RowDemand>,
    /// The chart itself as a CPU plays it (`cpu-play-v1`, `super::full`);
    /// none where it cannot be played whole.
    full: Option<Box<FullPlan>>,
}

fn chart_rows(song: &Song, index: usize) -> Result<Vec<Row>, String> {
    let chart = &song.charts[index];
    if chart.notes.len() > MAX_AUTOMATIC_ROWS * 4 {
        return Err("CPU chart exceeds the note limit".into());
    }
    let judge = Judge::new(&chart.notes, chart.timing(song), presets::itg().judge);
    let mut by_tick: BTreeMap<Tick, Row> = BTreeMap::new();
    for n in judge.notes().iter().filter(|n| n.judgeable) {
        if n.note.kind != NoteKind::Tap
            || n.note.lane >= 4
            || !n.seconds.is_finite()
            || n.seconds.abs() > 86400.
        {
            return Err("CPU expects finite tap-only 4K notes within one day".into());
        }
        let row = by_tick.entry(n.note.tick).or_insert(Row {
            seconds: n.seconds,
            mask: 0,
        });
        let bit = 1 << n.note.lane;
        if row.mask & bit != 0 {
            return Err("Duplicate CPU lane at one row".into());
        }
        row.mask |= bit;
    }
    if by_tick.is_empty() || by_tick.len() > MAX_AUTOMATIC_ROWS {
        return Err(format!(
            "CPU needs 1–{MAX_AUTOMATIC_ROWS} playable tap rows"
        ));
    }
    let mut rows: Vec<_> = by_tick.into_values().collect();
    // Playback follows judged seconds, including simultaneous rows after warps
    // or source-specific timing; it need not follow increasing beat numbers.
    rows.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    Ok(rows)
}

impl Automatic {
    fn new(song: &Song, index: usize) -> Result<Self, String> {
        let projected = project(song, index)?;
        let rows = chart_rows(&projected, index)?;
        let frequency = frequencies(&rows);
        let count: u32 = rows.iter().map(|r| r.mask.count_ones()).sum();
        let mean = rows
            .iter()
            .zip(&frequency)
            .map(|(r, f)| r.mask.count_ones() as f64 * f)
            .sum::<f64>()
            / count as f64;
        let good = gaussian(9.);
        let reward = |q: &[f64; 91]| {
            q.iter()
                .zip(basis::REWARDS)
                .map(|(p, r)| p * r)
                .sum::<f64>()
        };
        // Same independent-note 95%-Wife analytical anchor as the research
        // baseline, now across the whole song. Input matching and the selected
        // game judge can change the actual score; this is not a score promise.
        let target = ((0.95 + 0.0001 * 2.75) / 0.9999 - (1. - mean) * reward(&good)) / mean;
        let (mut lo, mut hi) = (9., 80.);
        for _ in 0..32 {
            let mid = (lo + hi) * 0.5;
            if reward(&gaussian(mid)) > target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let width = (lo + hi) * 0.5;
        let bad = STRENGTHS.map(|s| gaussian((width / s).clamp(9., 80.)));
        let demand = personality::demands(&rows);
        let absolute = absolute::row_demands(&rows, &absolute::ABSOLUTE_V1);
        let full = FullPlan::new(song, index).ok().map(Box::new);
        Ok(Self {
            digest: song.chart_digest(index),
            rows,
            frequency,
            good,
            bad,
            width,
            demand,
            absolute,
            full,
        })
    }

    fn script(&self, level: usize, seed: u64) -> Result<Vec<Press>, String> {
        let bad = self.bad.get(level).ok_or("CPU strength must be 0..4")?;
        let scale = STRENGTHS[level].powi(-2);
        motor(&self.rows, seed, |i, u| {
            if u[2] < OMITTED {
                return None;
            }
            let f = (self.frequency[i] * scale).min(1.);
            let mut q = [0.; 91];
            for (j, p) in q[..90].iter_mut().enumerate() {
                *p = ((1. - f) * self.good[j] + f * bad[j]) * 0.9999;
            }
            q[90] = 0.0001;
            Some(offset(&basis::EDGES, &q, u[0], u[1]))
        })
    }

    fn skill_law(&self, row: usize, strength: f64, logs: &[f64; 7]) -> [f64; 91] {
        let ability = strength
            * self.demand[row]
                .iter()
                .zip(logs)
                .map(|(d, s)| d * s)
                .sum::<f64>()
                .exp();
        let frequency = (self.frequency[row] / ability.powi(2)).min(1.);
        let bad = gaussian((self.width / ability).clamp(9., 80.));
        let mut q = [0.; 91];
        for (i, p) in q[..90].iter_mut().enumerate() {
            *p = ((1. - frequency) * self.good[i] + frequency * bad[i]) * 0.9999;
        }
        q[90] = 0.0001;
        q
    }

    fn script_with_skills(
        &self,
        level: usize,
        seed: u64,
        skills: Skills,
    ) -> Result<Vec<Press>, String> {
        let strength = *STRENGTHS.get(level).ok_or("CPU strength must be 0..4")?;
        let logs = skills.log_values();
        let mut current = None;
        let mut q = [0.; 91];
        motor(&self.rows, seed, |i, u| {
            if u[2] < OMITTED {
                return None;
            }
            // Compute a law once per row, not once per chord note. Memory stays
            // bounded by the compact plan; no full-song probability matrix.
            if current != Some(i) {
                q = self.skill_law(i, strength, &logs);
                current = Some(i);
            }
            Some(offset(&basis::EDGES, &q, u[0], u[1]))
        })
    }

    /// A level beyond the frozen strengths: the same law per row at its
    /// strength, and its lapse: a note left out with the chance
    /// `level.lapse` times the row's frequency (bent by its skills as its
    /// ability is), on top of the omitted component. The draws are the
    /// motor's own three per note, so a level without lapse presses as
    /// [`Automatic::script_with_skills`] would at its strength.
    fn script_at(&self, level: Level, seed: u64, skills: Skills) -> Result<Vec<Press>, String> {
        let logs = skills.log_values();
        let mut current = None;
        let mut q = [0.; 91];
        let mut omit = OMITTED;
        motor(&self.rows, seed, |i, u| {
            if current != Some(i) {
                q = self.skill_law(i, level.strength, &logs);
                // Its skills' share of its ability at this row.
                let bend = self.demand[i]
                    .iter()
                    .zip(&logs)
                    .map(|(d, s)| d * s)
                    .sum::<f64>()
                    .exp();
                omit = OMITTED
                    + level.lapse * (self.frequency[i] / bend.powi(2)).min(1.) * (1. - OMITTED);
                current = Some(i);
            }
            (u[2] >= omit).then(|| offset(&basis::EDGES, &q, u[0], u[1]))
        })
    }
}

/// Research frequency recipe, with continuous history across the song:
/// closely spaced rows, lane repetitions and chords raise error frequency.
fn frequencies(rows: &[Row]) -> Vec<f64> {
    let mut last = [f64::NEG_INFINITY; 4];
    let mut previous = f64::NEG_INFINITY;
    rows.iter()
        .map(|r| {
            let density = ((0.30 - (r.seconds - previous)) / 0.20).clamp(0., 1.);
            let mut repeat: f64 = 0.;
            for (lane, t) in last.iter_mut().enumerate() {
                if r.mask & (1 << lane) != 0 {
                    repeat = repeat.max(((0.18 - (r.seconds - *t)) / 0.10).clamp(0., 1.));
                    *t = r.seconds;
                }
            }
            previous = r.seconds;
            let chord = ((r.mask.count_ones() - 1) as f64 / 2.).min(1.);
            0.1 + 0.9 * density.max(repeat).max(chord)
        })
        .collect()
}

// Standard normal upper-tail approximation (absolute error <8e-8). Only
// nonnegative arguments are needed, avoiding cancellation in the far tails.
fn tail(x: f64) -> f64 {
    let t = 1. / (1. + 0.2316419 * x);
    let polynomial = t
        * (0.319381530
            + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    (-0.5 * x * x).exp() * 0.3989422804014327 * polynomial
}

pub(super) fn gaussian(width_ms: f64) -> [f64; 91] {
    let sigma = width_ms / 1000.;
    let tails = basis::EDGES.map(|x| if x == 0. { 0.5 } else { tail(x.abs() / sigma) });
    let mut q = [0.; 91];
    for i in 0..88 {
        q[i] = if basis::EDGES[i] >= 0. {
            tails[i] - tails[i + 1]
        } else if basis::EDGES[i + 1] <= 0. {
            tails[i + 1] - tails[i]
        } else {
            1. - tails[i] - tails[i + 1]
        };
    }
    q[88] = tails[0];
    q[89] = tails[88];
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probability_mass_symmetry_and_stronger_error_ordering() {
        for width in [9., 20., 40., 80.] {
            let q = gaussian(width);
            assert!(q.iter().all(|p| *p >= 0. && p.is_finite()));
            assert!((q.iter().sum::<f64>() - 1.).abs() < 1e-12);
            for i in 0..44 {
                assert!((q[i] - q[87 - i]).abs() < 1e-12);
            }
        }
        assert!((tail(1.) - 0.15865525393145707).abs() < 8e-8);
    }

    #[test]
    fn history_tracks_repeated_lanes_and_recovers_after_gap() {
        let f = frequencies(&[
            Row {
                seconds: 0.,
                mask: 1,
            },
            Row {
                seconds: 0.12,
                mask: 1,
            },
            Row {
                seconds: 0.24,
                mask: 2,
            },
            Row {
                seconds: 3.,
                mask: 2,
            },
            Row {
                seconds: 4.,
                mask: 7,
            },
        ]);
        assert_eq!(f[0], 0.1);
        assert!(f[1] > 0.9 && f[2] > 0.9);
        assert_eq!(f[3], 0.1);
        assert_eq!(f[4], 1.);
    }
}
