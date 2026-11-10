//! Seven bounded skill multipliers and causal pattern-demand proxies. These
//! dimensions borrow MSD's vocabulary; they are not MSD measurements or weights.
use super::Row;

pub const SKILL_NAMES: [&str; 7] = [
    "Stream",
    "Jumpstream",
    "Handstream",
    "Stamina",
    "JackSpeed",
    "Chordjack",
    "Technical",
];

/// Thousandths keep equality, route round-trips and display bounded. Private
/// storage guarantees every multiplier is finite and in 0.5..=2.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skills([u16; 7]);

const PRESETS: [(&str, [u16; 7]); 6] = [
    ("balanced", [1000; 7]),
    ("stream", [1250, 1000, 1000, 1000, 800, 1000, 1000]),
    ("jacks", [800, 1000, 1000, 1000, 1250, 1000, 1000]),
    ("chords", [800, 1250, 1250, 1000, 800, 1250, 800]),
    ("stamina", [800, 1000, 1000, 1250, 1000, 1000, 1000]),
    ("technical", [800, 1000, 1000, 1000, 1000, 1000, 1250]),
];

impl Default for Skills {
    fn default() -> Self {
        Self([1000; 7])
    }
}

impl Skills {
    pub fn new(values: [f64; 7]) -> Result<Self, String> {
        let mut stored = [0; 7];
        for (i, value) in values.into_iter().enumerate() {
            let scaled = value * 1000.;
            if !value.is_finite()
                || !(0.5..=2.).contains(&value)
                || (scaled - scaled.round()).abs() > 1e-9
            {
                return Err(
                    "CPU skills need seven multipliers from 0.5 to 2, in 0.001 steps".into(),
                );
            }
            stored[i] = scaled.round() as u16;
        }
        Ok(Self(stored))
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        if let Some((_, values)) = PRESETS.iter().find(|(name, _)| *name == value) {
            return Ok(Self(*values));
        }
        if value.len() > 192 {
            return Err("CPU skill vector is too long".into());
        }
        let parts: Vec<_> = value.split(',').collect();
        if parts.len() != 7 {
            return Err("CPU skills: balanced, stream, jacks, chords, stamina, technical, or seven comma-separated multipliers".into());
        }
        let mut values = [0.; 7];
        for (i, part) in parts.into_iter().enumerate() {
            values[i] = part
                .trim()
                .parse()
                .map_err(|_| "Invalid CPU skill multiplier")?;
        }
        Self::new(values)
    }

    pub fn values(self) -> [f64; 7] {
        self.0.map(|v| f64::from(v) / 1000.)
    }

    pub fn vector(self) -> String {
        self.values().map(|v| v.to_string()).join(",")
    }

    pub(super) fn log_values(self) -> [f64; 7] {
        self.values().map(f64::ln)
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
