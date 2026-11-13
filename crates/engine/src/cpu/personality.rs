//! Eight bounded skill multipliers: seven pattern axes with causal
//! pattern-demand proxies, which borrow MSD's vocabulary (they are not MSD
//! measurements or weights), and Evasion, how a CPU fares against a
//! battle's shocks (`super::mines`, `MINE_RESPONSE_V1`): a model of a
//! CPU, never a measure of a person.
use super::Row;

pub const SKILL_NAMES: [&str; 8] = [
    "Stream",
    "Jumpstream",
    "Handstream",
    "Stamina",
    "JackSpeed",
    "Chordjack",
    "Technical",
    "Evasion",
];

/// The pattern axes (the first seven of [`SKILL_NAMES`]): what a chart's
/// rows demand. The eighth, Evasion, is read by the response to shocks.
pub const PATTERN_AXES: usize = 7;

/// Thousandths keep equality, route round-trips and display bounded. Private
/// storage guarantees every multiplier is finite and in 0.5..=2.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skills([u16; 8]);

/// The presets. Evasion stays 1 but where the character suggests
/// another: `technical` dodges better (1.25), `chords` mashes onto more
/// shocks (0.8); `evasive` and `reckless` are balanced players who dodge
/// shocks well (1.6) and step on them (0.6).
const PRESETS: [(&str, [u16; 8]); 8] = [
    ("balanced", [1000; 8]),
    ("stream", [1250, 1000, 1000, 1000, 800, 1000, 1000, 1000]),
    ("jacks", [800, 1000, 1000, 1000, 1250, 1000, 1000, 1000]),
    ("chords", [800, 1250, 1250, 1000, 800, 1250, 800, 800]),
    ("stamina", [800, 1000, 1000, 1250, 1000, 1000, 1000, 1000]),
    ("technical", [800, 1000, 1000, 1000, 1000, 1000, 1250, 1250]),
    ("evasive", [1000, 1000, 1000, 1000, 1000, 1000, 1000, 1600]),
    ("reckless", [1000, 1000, 1000, 1000, 1000, 1000, 1000, 600]),
];

/// The presets' names, as [`Skills::parse`] takes them.
pub const PRESET_NAMES: [&str; 8] = [
    "balanced",
    "stream",
    "jacks",
    "chords",
    "stamina",
    "technical",
    "evasive",
    "reckless",
];

impl Default for Skills {
    fn default() -> Self {
        Self([1000; 8])
    }
}

impl Skills {
    /// The seven pattern multipliers, Evasion 1.
    pub fn new(values: [f64; 7]) -> Result<Self, String> {
        Self::from_values(&values)
    }

    /// Seven multipliers (the pattern axes, Evasion 1) or eight.
    pub fn from_values(values: &[f64]) -> Result<Self, String> {
        if !(PATTERN_AXES..=SKILL_NAMES.len()).contains(&values.len()) {
            return Err("CPU skills need seven or eight multipliers".into());
        }
        let mut stored = [1000; 8];
        for (i, value) in values.iter().copied().enumerate() {
            let scaled = value * 1000.;
            if !value.is_finite()
                || !(0.5..=2.).contains(&value)
                || (scaled - scaled.round()).abs() > 1e-9
            {
                return Err(
                    "CPU skills need seven or eight multipliers from 0.5 to 2, in 0.001 steps"
                        .into(),
                );
            }
            stored[i] = scaled.round() as u16;
        }
        Ok(Self(stored))
    }

    /// The same with Evasion `evasion` (0.5 to 2).
    pub fn with_evasion(self, evasion: f64) -> Result<Self, String> {
        let mut v = self.values();
        v[PATTERN_AXES] = evasion;
        Self::from_values(&v)
    }

    /// The eighth axis: how a CPU fares against shocks, 1 neutral.
    pub fn evasion(self) -> f64 {
        f64::from(self.0[PATTERN_AXES]) / 1000.
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        if let Some((_, values)) = PRESETS.iter().find(|(name, _)| *name == value) {
            return Ok(Self(*values));
        }
        if value.len() > 192 {
            return Err("CPU skill vector is too long".into());
        }
        let parts: Vec<_> = value.split(',').collect();
        if !(PATTERN_AXES..=SKILL_NAMES.len()).contains(&parts.len()) {
            return Err("CPU skills: balanced, stream, jacks, chords, stamina, technical, evasive, reckless, or seven or eight comma-separated multipliers".into());
        }
        let mut values = Vec::with_capacity(parts.len());
        for part in parts {
            values.push(
                part.trim()
                    .parse()
                    .map_err(|_| "Invalid CPU skill multiplier")?,
            );
        }
        Self::from_values(&values)
    }

    /// All eight, Evasion last.
    pub fn values(self) -> [f64; 8] {
        self.0.map(|v| f64::from(v) / 1000.)
    }

    /// The multipliers as [`Skills::parse`] takes them: seven where
    /// Evasion is 1 (as every vector was before it), else eight.
    pub fn vector(self) -> String {
        let v = self.values();
        let n = if self.0[PATTERN_AXES] == 1000 {
            PATTERN_AXES
        } else {
            v.len()
        };
        v[..n]
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The pattern axes' logarithms (what the rows' demands bend).
    pub(super) fn log_values(self) -> [f64; 7] {
        let v = self.values();
        std::array::from_fn(|i| v[i].ln())
    }
}

impl std::fmt::Display for Skills {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some((name, _)) = PRESETS.iter().find(|(_, v)| *v == self.0) {
            f.write_str(name)
        } else {
            f.write_str(&self.vector())
        }
    }
}

/// Nonnegative demand weights sum to at most one. Unused weight represents
/// ordinary precision, affected by global strength but not skill specialization.
/// Density/repetition uses seconds, independent of a source's meter or labels.
pub(super) fn demands(rows: &[Row]) -> Vec<[f64; 7]> {
    let mut last = [f64::NEG_INFINITY; 4];
    let mut previous: Option<&Row> = None;
    let mut previous_dt = f64::INFINITY;
    let (mut notes, mut jumps, mut hands, mut load) = (0., 0., 0., 0.);
    rows.iter()
        .map(|row| {
            let dt = previous.map_or(f64::INFINITY, |p| row.seconds - p.seconds);
            let speed = ((0.30 - dt) / 0.20).clamp(0., 1.);
            let count = f64::from(row.mask.count_ones());
            let decay = (-dt / 1.5).exp();
            notes = notes * decay + count;
            jumps = jumps * decay + if count == 2. { count } else { 0. };
            hands = hands * decay + if count >= 3. { count } else { 0. };
            load = load * (-dt / 6.).exp() + count;
            let mut repeat: f64 = 0.;
            for (lane, when) in last.iter_mut().enumerate() {
                if row.mask & (1 << lane) != 0 {
                    repeat = repeat.max(((0.18 - (row.seconds - *when)) / 0.10).clamp(0., 1.));
                    *when = row.seconds;
                }
            }
            let irregular = if dt > 0. && dt <= 0.30 && previous_dt > 0. && previous_dt <= 0.30 {
                ((dt / previous_dt).ln().abs() / std::f64::consts::LN_2).min(1.)
            } else {
                0.
            };
            let (coordination, transition) = previous.map_or((0., 0.), |p| {
                let single_move = count == 1. && p.mask.count_ones() == 1 && p.mask != row.mask;
                let same_hand = (p.mask | row.mask) & 3 == 0 || (p.mask | row.mask) & 12 == 0;
                (
                    if single_move && same_hand { 1. } else { 0. },
                    if p.mask.count_ones() != row.mask.count_ones() {
                        1.
                    } else {
                        0.
                    },
                )
            });
            let chord_repeat = if count >= 2. { repeat } else { 0. };
            let mut demand = [
                speed * (1. - (jumps + hands) / notes).max(0.) * (1. - repeat),
                speed * jumps / notes * (1. - chord_repeat),
                speed * hands / notes * (1. - chord_repeat),
                speed * ((load - 12.) / 36.).clamp(0., 1.),
                if count == 1. { repeat } else { 0. },
                chord_repeat,
                speed * (0.6 * irregular + 0.4 * coordination + 0.3 * transition).min(1.),
            ];
            let total = demand.iter().sum::<f64>().max(1.);
            for v in &mut demand {
                *v /= total;
            }
            previous = Some(row);
            previous_dt = dt;
            demand
        })
        .collect()
}

#[cfg(test)]
mod tests;
