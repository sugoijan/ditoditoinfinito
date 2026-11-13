//! A CPU whose ability is absolute (`cpu-levels-v1`,
//! `docs/plans/cpu-player.md`, "An opponent's ten levels"): the same on
//! every chart, so an easy chart is played nearly perfectly by a middling
//! CPU and a chart above its head breaks it.
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
//! - **How it fails** past its limits: its spread widens in a straight
//!   line once the row's demand passes [`AbsoluteModel::onset`] of its
//!   limit, and it leaves notes out with a chance rising with how far the
//!   demand is above that ([`AbsoluteModel::lapse_linear`],
//!   [`AbsoluteModel::lapse_square`]), never a flat share.
//!
//! The shape of that fall-off (where it starts, how fast the spread
//! widens and the misses rise) was checked against one player's local
//! play history, which is not included; the levels' values are rungs
//! chosen by measurement over charts ([`CPU_LEVELS_V1`]), never a
//! person's skill rating.
//!
//! The motor is the same (three draws a note, 12 ms holds, the third
//! deciding a note left out); the timing law of a row is a mixture of two
//! normal laws on the plan's grid, cached by its spread in tenths of a
//! millisecond. The arithmetic is the plan's own (sums, products,
//! divisions, `ln` for the rhythm's irregularity and the normal tail's
//! `exp`), the same natively and in wasm.

use super::{OMITTED, Press, Row, Skills, basis, motor, offset};
use std::collections::BTreeMap;

/// What a CPU can do, in real units, the same on every chart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ability {
    /// How precisely it times a comfortable note: the spread (standard
    /// deviation) of its presses' offsets, milliseconds.
    pub spread_ms: f64,
    /// The notes a second it keeps up with before it degrades (the
    /// second before a row and the gap since the last row, single notes).
    pub speed: f64,
    /// The chance it leaves a note out where nothing is hard.
    pub floor: f64,
}

impl Ability {
    /// Within what a plan can be made for.
    pub fn validate(&self) -> Result<(), String> {
        if self.spread_ms.is_finite()
            && (1.0..=100.0).contains(&self.spread_ms)
            && self.speed.is_finite()
            && (0.5..=50.0).contains(&self.speed)
            && self.floor.is_finite()
            && (0.0..=0.5).contains(&self.floor)
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
    /// The demand (a share of the limit) from which its spread widens and
    /// it starts leaving notes out.
    pub onset: f64,
    /// The spread's widening per unit of demand past the onset.
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
}

/// `cpu-model-v1`'s shape (PROVISIONAL). The onset, the widening and the
/// lapse's rise were checked against one player's local play history
/// (not included): a spread flat up to about 0.85 of the limit, about
/// half again as wide at the limit, two and a half times at 1.3 times
/// it; notes left out from about one in a hundred at the limit to one in
/// twenty at 1.3 times it, rising on. The shares (a lane repeating at
/// 0.65 of the speed, sustained at 0.75) and the tail are from the same
/// check.
pub const ABSOLUTE_V1: AbsoluteModel = AbsoluteModel {
    window: 1.0,
    gap_share: 0.3,
    most_rate: 20.0,
    sustain: 8.0,
    stamina_share: 0.75,
    fatigue: 0.7,
    jack_share: 0.65,
    onset: 0.85,
    widen: 3.0,
    most_spread_ms: 150.0,
    technical: 0.35,
    lapse_linear: 0.067,
    lapse_square: 0.1,
    most_lapse: 0.95,
    tail_share: 0.12,
    tail_width: 2.5,
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
        });
        near += row.mask.count_ones();
        long += row.mask.count_ones();
        previous_gap = gap;
    }
    out
}

/// What a CPU of `a` with `skills` does at a row asking `d`: its spread
/// (milliseconds) and the chance it leaves a note out.
pub fn respond(a: &Ability, skills: &Skills, d: &RowDemand, m: &AbsoluteModel) -> (f64, f64) {
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
    let load = d.speed / speed + m.fatigue * over(d.sustain / stamina - 1.0);
    let lanes = d.lane / jack;
    let rhythm = 1.0 + m.technical * d.irregular / v[6];
    let spread =
        (a.spread_ms * (1.0 + m.widen * over(load - m.onset)) * rhythm).min(m.most_spread_ms);
    let x = over(load.max(lanes) - m.onset);
    let lapse = (a.floor + m.lapse_linear * x + m.lapse_square * x * x).min(m.most_lapse);
    (spread, lapse)
}

/// The timing law at a spread of `tenths` tenths of a millisecond: the
/// model's mixture on the plan's grid, the omitted component last.
fn law(tenths: u32, m: &AbsoluteModel) -> [f64; 91] {
    let width = f64::from(tenths) / 10.0;
    let near = super::automatic::gaussian(width);
    let far = super::automatic::gaussian(width * m.tail_width);
    let mut q = [0.0; 91];
    for (i, p) in q[..90].iter_mut().enumerate() {
        *p = ((1.0 - m.tail_share) * near[i] + m.tail_share * far[i]) * 0.9999;
    }
    q[90] = 0.0001;
    q
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
    let m = &ABSOLUTE_V1;
    let mut laws: BTreeMap<u32, [f64; 91]> = BTreeMap::new();
    let mut current = None;
    let mut q = [0.0; 91];
    let mut omit = OMITTED;
    motor(rows, seed, |i, u| {
        if current != Some(i) {
            let (spread, lapse) = respond(&a, &skills, &demands[i], m);
            let tenths = (spread * 10.0).round().max(10.0) as u32;
            q = *laws.entry(tenths).or_insert_with(|| law(tenths, m));
            omit = OMITTED + lapse * (1.0 - OMITTED);
            current = Some(i);
        }
        (u[2] >= omit).then(|| offset(&basis::EDGES, &q, u[0], u[1]))
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
        let a = Ability {
            spread_ms: 10.0,
            speed: 6.0,
            floor: 0.001,
        };
        let s = Skills::default();
        let at = |speed: f64| {
            respond(
                &a,
                &s,
                &RowDemand {
                    speed,
                    sustain: speed * 0.5,
                    lane: 0.0,
                    irregular: 0.0,
                    count: 1,
                },
                &m,
            )
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
        };
        assert!(respond(&a, &streamer, &d, &m).1 < respond(&a, &s, &d, &m).1);
        // A jack over its lane limit misses where the speed alone would not.
        let jack = RowDemand {
            speed: 4.0,
            sustain: 2.0,
            lane: 6.0,
            irregular: 0.0,
            count: 1,
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
