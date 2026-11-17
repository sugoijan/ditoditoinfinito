//! A CPU whose ability is absolute (`cpu-levels-v2` since round V,
//! `cpu-levels-v1` before; `docs/plans/cpu-player.md`, "An opponent's ten
//! levels"): the same on every chart, so an easy chart is played well by
//! a middling CPU and a chart above its head breaks it.
//!
//! - **Its abilities**, in real units ([`Ability`]): how precisely it
//!   times a comfortable note (`spread_ms`, a spread in milliseconds) and
//!   how fast it can go before it degrades (`speed`, notes a second; a
//!   lane repeating and a sustained stretch get their limits from it in
//!   the model's fixed shares, [`AbsoluteModel`]). The seven pattern axes
//!   of [`Skills`] multiply the limit their pattern meets (Stream,
//!   Jumpstream and Handstream the speed of a row of one, two or more
//!   notes; Stamina the sustained limit; JackSpeed and Chordjack a
//!   repeated lane's; Technical divides an irregular rhythm's widening).
//! - **A row's demand**, read from the chart in real units
//!   ([`RowDemand`]): the notes in the second before it and the gap since
//!   the row before (notes a second), the notes in the eight seconds
//!   before it (notes a second, sustained), how soon a lane repeats
//!   (presses a second on one lane), and how irregular the rhythm is. Never
//!   normalized by the chart's own average or peak; nothing is fitted per
//!   chart.
//! - **How it fails** as a row asks more: its spread widens in a straight
//!   line once the row's demand passes [`AbsoluteModel::spread_onset`] of
//!   its limit, and it leaves notes out with a chance rising with how far
//!   the demand is above [`AbsoluteModel::onset`]
//!   ([`AbsoluteModel::lapse_linear`], [`AbsoluteModel::lapse_square`]),
//!   never a flat share. Under `cpu-model-v2` ([`ABSOLUTE_V2`]) the
//!   spread widens well before notes are left out, as a person's hits get
//!   loose long before they miss.
//! - **Its own timing** (`cpu-model-v2`, [`Personal`]): a steady lean
//!   drawn from its seed (early or late, as poor calibration is) and a
//!   drift about it, a play's form, a burst of looser presses after a
//!   stumble and a little fatigue toward a song's end; `cpu-model-v1`
//!   ([`ABSOLUTE_V1`]) has none of these and presses as it always did.
//!
//! The shape (where the spread starts to widen, how fast, where misses
//! start, the lean, the bursts) was read from one player's local play
//! history, as aggregates; it is not included. The levels' values are
//! rungs fitted by measurement over charts to targets stated before the
//! fit ([`super::CPU_LEVELS_V2`]), never a person's skill rating.
//!
//! The motor is the same (three draws a note, 12 ms holds, the third
//! deciding a note left out); the timing law of a row is a mixture of two
//! normal laws on the plan's grid, cached by its spread in tenths of a
//! millisecond; `cpu-model-v2`'s own timing draws nothing from the
//! motor's stream (its draws are keyed by the seed). The arithmetic is the plan's own: sums, products,
//! divisions and square roots, correctly rounded and so the same natively
//! and in wasm, and `ln` for the rhythm's irregularity and the normal
//! tail's `exp` (with `powf` in the shock response, `super::mines`),
//! which are not: they come from the platform's maths library natively
//! and from Rust's port of musl's in wasm, and may differ in the last bit
//! (review finding engine-4). What they feed is quantized before it
//! decides anything (the spread to tenths of a millisecond, a press to
//! the microsecond, a draw compared to a chance), so a difference shows
//! only where a value falls within a bit of such a boundary; the online
//! CPUs' fixture (`ddi_session`'s `the_cpus_play_alike`) agreeing natively
//! and in wasm is one sample of that, not a proof. Where two devices do
//! differ, the room keeps the first report of each CPU's checkpoint and
//! counts the other (`ddi_room::live::LiveStage::cpu_mismatches`); the
//! shocks it commits are the same for every device either way.

use super::mines::{Purpose, draw};
use super::{OMITTED, Press, Row, Skills, basis, motor, offset};
use std::collections::BTreeMap;

/// What a CPU can do, in real units, the same on every chart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ability {
    /// How precisely it times a comfortable note: the spread (standard
    /// deviation) of its presses' offsets about its own lean,
    /// milliseconds.
    pub spread_ms: f64,
    /// The notes a second it keeps up with before it degrades (the
    /// second before a row and the gap since the last row, single notes).
    pub speed: f64,
    /// The chance it leaves a note out where nothing is hard.
    pub floor: f64,
    /// Its lean (`cpu-levels-v2`): the standard deviation of the steady
    /// offset each play of it has, drawn from its seed (early or late, as
    /// a person whose calibration is off is), milliseconds; 0 none.
    pub lean_ms: f64,
    /// The standard deviation of how far its lean wanders within a play
    /// (a knot every [`AbsoluteModel::drift_period`] seconds, a straight
    /// line between), milliseconds; 0 none.
    pub drift_ms: f64,
    /// The model it plays by ([`ABSOLUTE_V2`], [`ABSOLUTE_V1`], or a
    /// study's candidate).
    pub model: &'static AbsoluteModel,
}

impl Ability {
    /// An ability of `cpu-levels-v1` ([`ABSOLUTE_V1`]: no lean, no drift).
    pub const fn v1(spread_ms: f64, speed: f64, floor: f64) -> Ability {
        Ability {
            spread_ms,
            speed,
            floor,
            lean_ms: 0.0,
            drift_ms: 0.0,
            model: &ABSOLUTE_V1,
        }
    }

    /// Within what a plan can be made for.
    pub fn validate(&self) -> Result<(), String> {
        if self.spread_ms.is_finite()
            && (1.0..=100.0).contains(&self.spread_ms)
            && self.speed.is_finite()
            && (0.5..=50.0).contains(&self.speed)
            && self.floor.is_finite()
            && (0.0..=0.5).contains(&self.floor)
            && self.lean_ms.is_finite()
            && (0.0..=100.0).contains(&self.lean_ms)
            && self.drift_ms.is_finite()
            && (0.0..=100.0).contains(&self.drift_ms)
        {
            Ok(())
        } else {
            Err("CPU ability out of range".into())
        }
    }
}

/// The model's fixed shape: how a row's demand is read and how a CPU
/// fails past its limits. Named and versioned with the levels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbsoluteModel {
    pub id: &'static str,
    /// Seconds before a row whose notes count for its speed.
    pub window: f64,
    /// The gap since the row before's share of the speed (the rest the
    /// window's notes).
    pub gap_share: f64,
    /// The fastest rate a gap counts as (rows a second): rows at one
    /// time after a warp are not infinitely fast.
    pub most_rate: f64,
    /// Seconds before a row whose notes count for its sustained load.
    pub sustain: f64,
    /// The sustained limit as a share of the speed.
    pub stamina_share: f64,
    /// How much a sustained load over its limit adds to the row's demand
    /// (per unit over).
    pub fatigue: f64,
    /// A lane repeating: its limit (presses a second on one lane) as a
    /// share of the speed.
    pub jack_share: f64,
    /// The demand (a share of the limit) from which it starts leaving
    /// notes out (and from which a hold may be let go).
    pub onset: f64,
    /// The demand from which its spread widens (`cpu-levels-v2`: well
    /// before it leaves notes out; `cpu-levels-v1`: the same as `onset`).
    pub spread_onset: f64,
    /// The spread's widening per unit of demand past `spread_onset`.
    pub widen: f64,
    /// The widest spread, milliseconds.
    pub most_spread_ms: f64,
    /// An irregular rhythm's widening at its most (gaps changing by a
    /// factor of two or more), before Technical divides it.
    pub technical: f64,
    /// The chance of leaving a note out past the onset: this times the
    /// demand over it, plus `lapse_square` times its square.
    pub lapse_linear: f64,
    pub lapse_square: f64,
    /// The most that chance comes to.
    pub most_lapse: f64,
    /// The timing law: a share of presses from a law `tail_width` times
    /// as wide (a press now and then far off, as people's are).
    pub tail_share: f64,
    pub tail_width: f64,
    /// What a row played while holds or rolls are down asks more
    /// (`cpu-play-v1`, the full chart): its speed demand times `1 +
    /// held_load` for each lane held ([`RowDemand::held`]). A tap-only
    /// copy (`cpu-play-v0`) holds nothing, so it never counts there.
    pub held_load: f64,
    /// `cpu-levels-v2`'s person-like timing (all off in
    /// `cpu-levels-v1`, which then presses exactly as it did):
    /// - seconds between the knots of a play's drift ([`Ability::drift_ms`];
    ///   0: none);
    pub drift_period: f64,
    /// - a play's form: its spread times `1 + form × z` (`z` a standard
    ///   draw from the seed, kept within ±2, halved where it is below 0),
    ///   some plays much looser than others, a few a little tighter;
    pub form: f64,
    /// - after a stumble (a note left out, or a press more than
    ///   `stumble_ms` off its note as aimed) its spread is `1 + burst`
    ///   times as wide, easing back over `burst_seconds`, so loose
    ///   presses and breaks come in bursts;
    pub burst: f64,
    pub burst_seconds: f64,
    pub stumble_ms: f64,
    /// - its spread `1 + fatigue × p` times as wide at the share `p` of
    ///   the play gone (its first row to its last), a little looser
    ///   toward a song's end.
    pub fatigue_end: f64,
}

impl AbsoluteModel {
    /// The chance a note is left out `x` past the onset (unbounded): the
    /// model's failure curve over the level's `floor`.
    pub fn lapse_over(&self, floor: f64, x: f64) -> f64 {
        floor + self.lapse_linear * x + self.lapse_square * x * x
    }
}

/// `cpu-model-v1`'s shape (PROVISIONAL): the spread flat up to 0.85 of a
/// limit, then 1.45 times as wide at the limit and 2.35 times at 1.3
/// times it; a note left out with a chance of about 0.012 at the limit,
/// 0.05 at 1.3 times it, rising on; a lane's limit 0.65 of the speed,
/// the sustained one 0.75; a tail of 12% of presses 2.5 times as wide.
/// That shape was checked against one player's local play history, which
/// is not included.
pub const ABSOLUTE_V1: AbsoluteModel = AbsoluteModel {
    id: "cpu-model-v1",
    window: 1.0,
    gap_share: 0.3,
    most_rate: 20.0,
    sustain: 8.0,
    stamina_share: 0.75,
    fatigue: 0.7,
    jack_share: 0.65,
    onset: 0.85,
    spread_onset: 0.85,
    widen: 3.0,
    most_spread_ms: 150.0,
    technical: 0.35,
    lapse_linear: 0.067,
    lapse_square: 0.1,
    most_lapse: 0.95,
    tail_share: 0.12,
    tail_width: 2.5,
    held_load: 0.25,
    drift_period: 0.0,
    form: 0.0,
    burst: 0.0,
    burst_seconds: 0.0,
    stumble_ms: 0.0,
    fatigue_end: 0.0,
};

/// `cpu-model-v2`'s shape (`cpu-levels-v2`, PROVISIONAL; round V,
/// `docs/plans/cpu-player.md`, "Round V"): a row's demand read as
/// `cpu-model-v1` reads it (the plan's demands are made once, with
/// [`ABSOLUTE_V1`]'s); the spread widening from 0.65 of a limit, 1.35
/// times as wide at the limit and 1.65 times at 1.3 times it, while
/// notes are left out only from the limit (`cpu-model-v1`'s curve past
/// it); a thin tail (2% of presses three times as wide); a lean and a
/// drift of a CPU's own ([`Ability::lean_ms`], [`Ability::drift_ms`], a
/// knot every 16 s); a play's form (a draw's 0.35 looser, half that
/// tighter); a stumble (a note left out, or a press more than 90 ms off)
/// doubling the spread, easing back over two seconds; and a quarter
/// wider by a song's end. The shape was read from one player's local
/// play history (its aggregates; not included).
pub const ABSOLUTE_V2: AbsoluteModel = AbsoluteModel {
    id: "cpu-model-v2",
    window: 1.0,
    gap_share: 0.3,
    most_rate: 20.0,
    sustain: 8.0,
    stamina_share: 0.75,
    fatigue: 0.7,
    jack_share: 0.65,
    onset: 1.0,
    spread_onset: 0.65,
    widen: 1.0,
    most_spread_ms: 150.0,
    technical: 0.35,
    lapse_linear: 0.067,
    lapse_square: 0.1,
    most_lapse: 0.95,
    tail_share: 0.02,
    tail_width: 3.0,
    held_load: 0.25,
    drift_period: 16.0,
    form: 0.35,
    burst: 1.0,
    burst_seconds: 2.0,
    stumble_ms: 90.0,
    fatigue_end: 0.25,
};

/// What a row asks of a CPU, read from the chart in real units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowDemand {
    /// Notes a second: the window's notes before the row (a second's) and
    /// the gap since the row before, in the model's shares.
    pub speed: f64,
    /// Notes a second over the sustained window before the row.
    pub sustain: f64,
    /// Presses a second on its busiest lane (the gap since that lane's
    /// last note); 0 where none of its lanes was played before.
    pub lane: f64,
    /// How irregular the rhythm is, `0..=1`: the gaps before and after the
    /// last row changing by a factor of two or more is 1 (both within
    /// 0.3 s).
    pub irregular: f64,
    /// Its notes.
    pub count: u8,
    /// The lanes held down (holds and rolls begun before it and lasting
    /// past it) while it is played: 0 on a tap-only copy
    /// (`cpu-play-v0`), counted on the full chart (`cpu-play-v1`).
    pub held: u8,
}

/// Each row's demand (`rows` in time order).
pub(super) fn row_demands(rows: &[Row], m: &AbsoluteModel) -> Vec<RowDemand> {
    let mut out = Vec::with_capacity(rows.len());
    // Two windows' starts and their notes, kept as the rows go.
    let (mut near_from, mut near) = (0usize, 0u32);
    let (mut long_from, mut long) = (0usize, 0u32);
    let mut lanes = [f64::NEG_INFINITY; 4];
    let mut previous_gap = f64::INFINITY;
    for (i, row) in rows.iter().enumerate() {
        let t = row.seconds;
        while near_from < i && rows[near_from].seconds < t - m.window {
            near -= rows[near_from].mask.count_ones();
            near_from += 1;
        }
        while long_from < i && rows[long_from].seconds < t - m.sustain {
            long -= rows[long_from].mask.count_ones();
            long_from += 1;
        }
        let gap = if i == 0 {
            f64::INFINITY
        } else {
            t - rows[i - 1].seconds
        };
        let rate = if gap > 0.0 {
            (1.0 / gap).min(m.most_rate)
        } else {
            m.most_rate
        };
        let speed = (1.0 - m.gap_share) * f64::from(near) / m.window + m.gap_share * rate;
        let mut lane: f64 = 0.0;
        for (l, last) in lanes.iter_mut().enumerate() {
            if row.mask & (1 << l) != 0 {
                if last.is_finite() {
                    let since = t - *last;
                    let r = if since > 0.0 {
                        (1.0 / since).min(m.most_rate)
                    } else {
                        m.most_rate
                    };
                    lane = lane.max(r);
                }
                *last = t;
            }
        }
        let irregular = if gap > 0.0 && gap <= 0.30 && previous_gap > 0.0 && previous_gap <= 0.30
        {
            ((gap / previous_gap).ln().abs() / std::f64::consts::LN_2).min(1.0)
        } else {
            0.0
        };
        out.push(RowDemand {
            speed,
            sustain: f64::from(long) / m.sustain,
            lane,
            irregular,
            count: row.mask.count_ones() as u8,
            held: 0,
        });
        near += row.mask.count_ones();
        long += row.mask.count_ones();
        previous_gap = gap;
    }
    out
}

/// What a CPU of `a` with `skills` does at a row asking `d`: its spread
/// (milliseconds), the chance it leaves a note out and the larger of its
/// two loads ([`loads`]).
pub fn respond(
    a: &Ability,
    skills: &Skills,
    d: &RowDemand,
    m: &AbsoluteModel,
) -> (f64, f64, f64) {
    let v = skills.values();
    let (load, lanes) = loads(a, skills, d, m);
    let over = |x: f64| if x > 0.0 { x } else { 0.0 };
    let rhythm = 1.0 + m.technical * d.irregular / v[6];
    let spread = (a.spread_ms * (1.0 + m.widen * over(load - m.spread_onset)) * rhythm)
        .min(m.most_spread_ms);
    let most = load.max(lanes);
    let lapse = m.lapse_over(a.floor, over(most - m.onset)).min(m.most_lapse);
    (spread, lapse, most)
}

/// What a row asking `d` is to a CPU of `a` with `skills`: its load
/// (its speed demand over its speed limit, more where holds are down,
/// plus its fatigue past its sustained limit) and its lane load (its
/// busiest lane over its lane limit); from 0.85 of either it degrades.
pub fn loads(a: &Ability, skills: &Skills, d: &RowDemand, m: &AbsoluteModel) -> (f64, f64) {
    let v = skills.values();
    let axis = match d.count {
        0 | 1 => v[0],
        2 => v[1],
        _ => v[2],
    };
    let speed = a.speed * axis;
    let stamina = a.speed * m.stamina_share * v[3];
    let jack = a.speed * m.jack_share * if d.count <= 1 { v[4] } else { v[5] };
    let over = |x: f64| if x > 0.0 { x } else { 0.0 };
    // Nothing held (every tap-only copy's row): the speed as it was.
    let asked = if d.held == 0 {
        d.speed
    } else {
        d.speed * (1.0 + m.held_load * f64::from(d.held))
    };
    let load = asked / speed + m.fatigue * over(d.sustain / stamina - 1.0);
    (load, d.lane / jack)
}

/// The timing law at a spread of `tenths` tenths of a millisecond: the
/// model's mixture on the plan's grid, the omitted component last.
pub(super) fn law(tenths: u32, m: &AbsoluteModel) -> [f64; 91] {
    let width = f64::from(tenths) / 10.0;
    let near = super::automatic::gaussian(width);
    let far = super::automatic::gaussian(width * m.tail_width);
    super::automatic::mixture(&near, &far, m.tail_share)
}

/// A play's own timing (`cpu-levels-v2`; nothing at all under
/// `cpu-levels-v1`): its lean and the drift about it, its form, a little
/// fatigue toward its end, and the burst of looser presses after a
/// stumble. Every draw is keyed by the seed ([`draw`]), never taken from
/// the motor's stream, and the arithmetic is sums, products, divisions
/// and a square root, so it is the same on every device.
pub(super) struct Personal {
    m: &'static AbsoluteModel,
    /// Seconds.
    lean: f64,
    drift: Vec<f64>,
    from: f64,
    span: f64,
    form: f64,
    /// When it last stumbled.
    stumbled: f64,
}

/// The farthest a press is aimed from its note (the timing law's grid's
/// own bound, `basis::EDGES`), seconds.
const AIM_BOUND: f64 = 0.18;

/// A standard draw from `seed` for `purpose` (`extra` its own key): the
/// sum of four keyed uniforms, centered and scaled (Irwin–Hall), within
/// `±most`.
fn standard(seed: u64, purpose: Purpose, extra: u64, most: f64) -> f64 {
    let s: f64 = (0..4).map(|k| draw(seed, 0, 0, purpose, extra * 4 + k)).sum();
    ((s - 2.0) * 3.0f64.sqrt()).clamp(-most, most)
}

impl Personal {
    /// The timing of a play of `a` with `seed` whose rows run from
    /// `first` to `last` seconds.
    pub(super) fn new(a: &Ability, seed: u64, first: f64, last: f64) -> Personal {
        let m = a.model;
        let span = (last - first).max(1.0);
        let lean = if a.lean_ms > 0.0 {
            (a.lean_ms * standard(seed, Purpose::Lean, 0, 2.5) * 10.0).round() / 10_000.0
        } else {
            0.0
        };
        let drift = if a.drift_ms > 0.0 && m.drift_period > 0.0 {
            let knots = (span / m.drift_period) as u64 + 2;
            (0..knots)
                .map(|k| (a.drift_ms * standard(seed, Purpose::Drift, k, 2.5) * 10.0).round() / 10_000.0)
                .collect()
        } else {
            Vec::new()
        };
        let form = if m.form > 0.0 {
            // Skewed: a bad day is looser by the whole draw, a good one
            // tighter by half of it.
            let z = standard(seed, Purpose::Form, 0, 2.0);
            1.0 + m.form * if z > 0.0 { z } else { z / 2.0 }
        } else {
            1.0
        };
        Personal {
            m,
            lean,
            drift,
            from: first,
            span,
            form,
            stumbled: f64::NEG_INFINITY,
        }
    }

    /// Where its presses aim at `t` against the note: its lean and the
    /// drift about it there, seconds.
    pub(super) fn shift(&self, t: f64) -> f64 {
        if self.drift.is_empty() {
            return self.lean;
        }
        let x = ((t - self.from) / self.m.drift_period).max(0.0);
        let k = (x as usize).min(self.drift.len() - 2);
        let f = (x - k as f64).min(1.0);
        self.lean + self.drift[k] + (self.drift[k + 1] - self.drift[k]) * f
    }

    /// A press `off` seconds off its note by the timing law, aimed at
    /// `t`: with its lean and drift, never more than the law's own
    /// bound (180 ms) off its note, as every press of `cpu-levels-v1`
    /// was (a press is judged against its note or left out, never a
    /// stray one).
    pub(super) fn aim(&self, t: f64, off: f64) -> f64 {
        if self.lean == 0.0 && self.drift.is_empty() {
            return off;
        }
        (self.shift(t) + off).clamp(-AIM_BOUND, AIM_BOUND)
    }

    /// How much wider than the row asks its spread is at `t`: its form,
    /// its fatigue so far and a stumble's burst.
    pub(super) fn factor(&self, t: f64) -> f64 {
        let m = self.m;
        let mut f = self.form;
        if m.fatigue_end > 0.0 {
            f *= 1.0 + m.fatigue_end * ((t - self.from) / self.span).clamp(0.0, 1.0);
        }
        if m.burst > 0.0 && t >= self.stumbled && t - self.stumbled < m.burst_seconds {
            f *= 1.0 + m.burst * (1.0 - (t - self.stumbled) / m.burst_seconds);
        }
        f
    }

    /// A note at `t` played: left out (`None`) or pressed `off` seconds
    /// off its note (its lean and drift included). A stumble starts a
    /// burst.
    pub(super) fn played(&mut self, t: f64, off: Option<f64>) {
        if self.m.burst > 0.0 && off.is_none_or(|o| o.abs() * 1000.0 > self.m.stumble_ms) {
            self.stumbled = t;
        }
    }
}

/// The timing law's key at `spread` milliseconds.
pub(super) fn tenths(spread: f64) -> u32 {
    (spread * 10.0).round().max(10.0) as u32
}

/// The presses of a CPU of `a` with `skills` and `seed` on `rows` (their
/// demands `demands`).
pub(super) fn script(
    rows: &[Row],
    demands: &[RowDemand],
    a: Ability,
    seed: u64,
    skills: Skills,
) -> Result<Vec<Press>, String> {
    a.validate()?;
    let m = a.model;
    let mut laws: BTreeMap<u32, [f64; 91]> = BTreeMap::new();
    let mut current = None;
    let mut q = [0.0; 91];
    let mut omit = OMITTED;
    let mut me = Personal::new(
        &a,
        seed,
        rows.first().map_or(0.0, |r| r.seconds),
        rows.last().map_or(0.0, |r| r.seconds),
    );
    motor(rows, seed, |i, u| {
        let t = rows[i].seconds;
        if current != Some(i) {
            let (spread, lapse, _) = respond(&a, &skills, &demands[i], m);
            let key = tenths(spread * me.factor(t));
            q = *laws.entry(key).or_insert_with(|| law(key, m));
            omit = OMITTED + lapse * (1.0 - OMITTED);
            current = Some(i);
        }
        let off = (u[2] >= omit).then(|| me.aim(t, offset(&basis::EDGES, &q, u[0], u[1])));
        me.played(t, off);
        off
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(gap: f64, n: usize, mask: u8) -> Vec<Row> {
        (0..n)
            .map(|i| Row {
                seconds: i as f64 * gap,
                mask: if mask == 0 { 1 << (i % 4) } else { mask },
            })
            .collect()
    }

    #[test]
    fn a_row_s_demand_is_in_real_units() {
        let m = ABSOLUTE_V1;
        // A steady stream of single notes, ten a second: about ten.
        let d = row_demands(&stream(0.1, 200, 0), &m);
        let late = d[150];
        assert!((late.speed - 10.0).abs() < 0.8, "{late:?}");
        assert!((late.sustain - 10.0).abs() < 0.3, "{late:?}");
        // Each lane every 0.4 s.
        assert!((late.lane - 2.5).abs() < 1e-9, "{late:?}");
        assert_eq!(late.irregular, 0.0);
        // The same notes at half the rate: half the demand.
        let slow = row_demands(&stream(0.2, 200, 0), &m)[150];
        assert!((slow.speed - late.speed / 2.0).abs() < 0.8, "{slow:?}");
        // A jack: one lane five times a second.
        let jack = row_demands(&stream(0.2, 50, 1), &m)[30];
        assert!((jack.lane - 5.0).abs() < 1e-9);
        // The first row asks nothing of speed but its gap's cap share.
        assert_eq!(d[0].lane, 0.0);
        // Rows at one time (a warp) are not infinitely fast.
        let warp = [
            Row {
                seconds: 1.0,
                mask: 1,
            },
            Row {
                seconds: 1.0,
                mask: 1,
            },
        ];
        let w = row_demands(&warp, &m);
        assert!(w[1].speed.is_finite() && w[1].lane <= m.most_rate);
        // Gaps doubling: irregular.
        let rows = [0.0, 0.1, 0.3, 0.4]
            .map(|s| Row {
                seconds: s,
                mask: 1,
            })
            .to_vec();
        let d = row_demands(&rows, &m);
        assert!(d[2].irregular > 0.999, "{:?}", d[2]);
        assert_eq!(d[1].irregular, 0.0);
    }

    #[test]
    fn it_fails_past_its_limits_and_not_before() {
        let m = ABSOLUTE_V1;
        let a = Ability::v1(10.0, 6.0, 0.001);
        let s = Skills::default();
        let at = |speed: f64| {
            let (spread, lapse, _) = respond(
                &a,
                &s,
                &RowDemand {
                    speed,
                    sustain: speed * 0.5,
                    lane: 0.0,
                    irregular: 0.0,
                    count: 1,
                    held: 0,
                },
                &m,
            );
            (spread, lapse)
        };
        // Comfortable: its spread and its floor.
        assert_eq!(at(3.0), (10.0, 0.001));
        assert_eq!(at(5.0), (10.0, 0.001));
        // At its limit, a little wider; past it, much wider and missing.
        let (s1, l1) = at(6.0);
        let (s2, l2) = at(8.0);
        let (s3, l3) = at(18.0);
        assert!(s1 > 10.0 && s1 < 16.0 && l1 > 0.001 && l1 < 0.03, "{s1} {l1}");
        assert!(s2 > s1 && l2 > l1, "{s2} {l2}");
        assert!(s3 > s2 && l3 > 0.4, "{s3} {l3}");
        assert!(at(500.0).0 <= m.most_spread_ms && at(500.0).1 <= m.most_lapse);
        // A skill multiplies its limit: a streamer keeps up where a
        // balanced CPU does not.
        let streamer = Skills::new([2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]).unwrap();
        let d = RowDemand {
            speed: 8.0,
            sustain: 2.0,
            lane: 0.0,
            irregular: 0.0,
            count: 1,
            held: 0,
        };
        assert!(respond(&a, &streamer, &d, &m).1 < respond(&a, &s, &d, &m).1);
        // A jack over its lane limit misses where the speed alone would not.
        let jack = RowDemand {
            speed: 4.0,
            sustain: 2.0,
            lane: 6.0,
            irregular: 0.0,
            count: 1,
            held: 0,
        };
        assert!(respond(&a, &s, &jack, &m).1 > 0.01);
        let jacker = Skills::new([1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0]).unwrap();
        assert!(respond(&a, &jacker, &jack, &m).1 < respond(&a, &s, &jack, &m).1);
    }

    #[test]
    fn the_law_is_a_probability_on_the_grid() {
        for tenths in [10, 95, 300, 1500] {
            let q = law(tenths, &ABSOLUTE_V1);
            assert!(q.iter().all(|p| *p >= 0.0 && p.is_finite()));
            assert!((q.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        }
    }
}
