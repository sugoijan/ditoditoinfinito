//! Expected raw Accuracy lost when Technical alone is weakened at the chart's displayed level.
//! Propagate the frozen timing laws through per-lane head assignment and mine locks.
use super::{features::Chart, prior::erf};
use ddi_engine::cpu::HOLD_MODEL_V1 as HOLD;
use ddi_engine::{
    cpu::{CPU_LEVELS_V2, MINE_RESPONSE_V1, RowDemand, Skills, TIMING_EDGES, row_response},
    rules::ddi::{self, Difficulty},
};
use std::collections::{BTreeMap, HashMap, VecDeque};
mod head_assignment;
mod timing;
mod weights;
use head_assignment::{PurePlan, apply_pure, pure_plans};
use timing::{Law, TimingGrids, laws};

const BOUND: f64 = 0.18;
#[derive(Default)]
struct Lane {
    heads: Vec<f64>,
    mines: Vec<f64>,
}
struct Geometry {
    lanes: [Lane; 4],
    rows: Vec<(f64, RowDemand)>,
}

#[derive(Clone, Copy, Debug)]
enum Source {
    Primary = 0,
    Deliberate = 1,
    Roll = 2,
    Regrab = 3,
    Held = 4,
}
#[derive(Clone, Copy)]
enum Timing {
    Motor(usize),
    Uniform,
    Fixed,
}
#[derive(Clone, Copy)]
struct Event {
    at: f64,
    source: Source,
    timing: Timing,
    emit: f64,
    head: usize,
    mine: usize,
    short: bool,
}
#[derive(Clone, Copy, Default)]
struct Meta {
    end: f64,
    roll: bool,
}
type State = BTreeMap<(usize, usize), f64>;
fn credit(s: &mut State, k: (usize, usize), p: f64) {
    if p > 0. {
        *s.entry(k).or_default() += p;
    }
}
fn row(g: &Geometry, t: f64) -> usize {
    g.rows.partition_point(|r| r.0 <= t).saturating_sub(1)
}
fn interval(l: &Law, lo: f64, hi: f64) -> f64 {
    if hi <= lo {
        return 0.;
    }
    let mass = l.raw_edges_cdf(hi - l.shift) - l.raw_edges_cdf(lo - l.shift)
        + l.atoms()
            .into_iter()
            .filter(|(x, _)| *x >= lo && *x < hi)
            .map(|(_, p)| p)
            .sum::<f64>();
    (1. - l.omit) * mass.clamp(0., 1.)
}
// Drop probabilities shared by hold survival and the judge's expected hold credit.
struct HoldDrops {
    masks: Vec<u8>,
    per_row: Vec<[f64; 4]>,
    floor: f64,
}

// Probability a planned long note still runs at t. Conditional survival over intervening
// rows and full seconds, independent of timing offset and previous lane-lock outcomes.
fn survival(g: &Geometry, start: f64, t: f64, lane: usize, drops: &HoldDrops, ls: &[Law]) -> f64 {
    let first = row(g, start);
    let end = g.rows.partition_point(|r| r.0 <= t);
    let mut p = 1. - ls[first].omit;
    for j in first + 1..end {
        if drops.masks[j] & (1 << lane) == 0 {
            p *= 1. - drops.per_row[j][lane];
        }
    }
    p * (1. - drops.floor).powi((t - start).max(0.).floor() as i32)
}
fn repeated(p: f64, n: usize) -> f64 {
    if n == 1 {
        p
    } else {
        1. - (1. - p).powf(n as f64)
    }
}
fn events(
    g: &Geometry,
    lane: usize,
    level: usize,
    ls: &[Law],
    meta: &[Meta],
    drops: &HoldDrops,
    mult: &[usize],
) -> Vec<Event> {
    let l = &g.lanes[lane];
    let hs = &l.heads;
    let ms = &l.mines;
    let cls = |at: f64, reach: f64| {
        let m = ms.partition_point(|&v| v < at - reach);
        m < ms.len() && ms[m] <= at + reach
    };
    let mut ev = hs
        .iter()
        .enumerate()
        .map(|(i, &t)| Event {
            at: t,
            source: Source::Primary,
            timing: Timing::Motor(row(g, t)),
            emit: 1.,
            head: i,
            mine: usize::MAX,
            short: meta[i].end <= t || meta[i].roll,
        })
        .collect::<Vec<_>>();
    if ms.is_empty() {
        return ev;
    }
    let alive = |i: usize, t: f64| survival(g, hs[i], t, lane, drops, ls);
    // Evasion is 1 in both paired conditions, so response(hit) is exactly the table value.
    let m = &MINE_RESPONSE_V1;
    let hit = m.table.response(m.table.avoidance(level)).hit;
    for (k, &t) in ms.iter().enumerate() {
        let j = hs.partition_point(|&h| h < t);
        let prev = j.checked_sub(1);
        let following = hs.get(j).copied().unwrap_or(f64::INFINITY);
        let beside =
            prev.is_some_and(|i| hs[i] >= t - m.lane_window) || following <= t + m.lane_window;
        let mut decided = 0.;
        if let Some(i) = prev {
            let end = meta[i].end;
            let stop = (end + HOLD.hold_after).min(following - HOLD.release_before);
            if end > hs[i] && t <= stop {
                decided = alive(i, t);
                if t < end && !meta[i].roll {
                    // If the first 12 ms of the head overlap the preparation boundary, the
                    // motor cannot lift for the mine. Test the same boundary with its law.
                    let l = &ls[row(g, hs[i])];
                    let late = interval(l, t - HOLD.mine_before - 0.012 - hs[i], t - hs[i])
                        / (1. - l.omit).max(1e-12);
                    let down =
                        (1. - interval(l, t - hs[i], 1.) / (1. - l.omit).max(1e-12)).clamp(0., 1.);
                    let held = (hit * down + (1. - hit) * late).clamp(0., 1.);
                    if held > 0. {
                        ev.push(Event {
                            at: t,
                            source: Source::Held,
                            timing: Timing::Fixed,
                            emit: repeated(decided * held, mult[k]),
                            head: i,
                            mine: k,
                            short: false,
                        });
                    }
                    let grab = t + HOLD.mine_after;
                    if grab + 0.012 <= stop && cls(grab, ddi::MINE_WINDOW + 0.012) {
                        ev.push(Event {
                            at: grab,
                            source: Source::Regrab,
                            timing: Timing::Fixed,
                            emit: alive(i, grab) * (1. - hit) * (1. - late),
                            head: i,
                            mine: k,
                            short: true,
                        });
                    }
                }
            }
        }
        if beside && decided < 1. {
            ev.push(Event {
                at: t,
                source: Source::Deliberate,
                timing: Timing::Uniform,
                emit: repeated(hit * (1. - decided), mult[k]),
                head: usize::MAX,
                mine: k,
                short: true,
            });
        }
    }
    for (i, md) in meta.iter().enumerate().filter(|(_, d)| d.roll) {
        let t = hs[i];
        let last = md
            .end
            .min(hs.get(i + 1).copied().unwrap_or(f64::INFINITY) - HOLD.roll_stop);
        let clear = |s: f64| !cls(s, HOLD.mine_clear - 1e-10);
        let mut taps = Vec::new();
        let mut s = t + HOLD.roll_interval;
        while s < last {
            if clear(s) {
                taps.push(s);
            }
            s += HOLD.roll_interval;
        }
        let first = ms.partition_point(|v| *v <= t);
        for &v in ms[first..].iter().take_while(|&&v| v < last) {
            for s in [v - 0.1, v + 0.1] {
                if s > t && s < last && clear(s) {
                    taps.push(s);
                }
            }
        }
        taps.sort_by(f64::total_cmp);
        taps.dedup_by(|b, a| *b - *a < 0.06);
        for s in taps {
            // clear(actual release) guarantees no direct roll-tap hit, and its 12 ms
            // duration is smaller than the clearance. No hazard can initiate a lock.
            if cls(s, BOUND + HOLD.mine_clear) {
                ev.push(Event {
                    at: s,
                    source: Source::Roll,
                    timing: Timing::Motor(row(g, s)),
                    emit: alive(i, s),
                    head: i,
                    mine: usize::MAX,
                    short: true,
                });
            }
        }
    }
    ev.sort_by(|a, b| {
        a.at.total_cmp(&b.at)
            .then((a.source as u8).cmp(&(b.source as u8)))
    });
    ev
}
// Exclude overlaps of an introduced deliberate mine press with motor presses. For a long
// note whose body the motor already decided, its survival was used to thin that proposal.
fn free(lane: &Lane, meta: &[Meta], ls: &[Law], g: &Geometry, at: f64, mine: f64) -> f64 {
    let hs = &lane.heads;
    let first = hs.partition_point(|&t| t < at - BOUND - 0.0122);
    let mut p = 1.;
    for i in first..hs.len() {
        let t = hs[i];
        if t > at + BOUND + 0.0122 {
            break;
        }
        let law = &ls[row(g, t)];
        let cap = (meta[i].end + HOLD.hold_after)
            .min(hs.get(i + 1).copied().unwrap_or(f64::INFINITY) - HOLD.release_before);
        // A hold already decided the mine if it survives this far. Conditional on an
        // unplanned/dropped hold, do not count full scheduled occupancy a second time.
        let long = meta[i].end > t
            && !meta[i].roll
            && (mine <= t || mine > cap + 1e-9)
            && at < cap + 0.0001;
        let lo = if long { -1. } else { at - t - 0.0121 };
        p *= 1. - interval(law, lo, at - t + 0.0121);
    }
    p
}
// Expected DDI units if this primary press lands on its own long head.
// The motor's row drops and floor checks stop a hold/roll, then the judge grants its release grace.
fn body_worth(
    g: &Geometry,
    lane: usize,
    idx: usize,
    md: Meta,
    drops: &HoldDrops,
    level: usize,
    mult: &[usize],
) -> (f64, f64) {
    let l = &g.lanes[lane];
    let start = l.heads[idx];
    let end = md.end;
    if end <= start {
        return (0., 0.);
    }
    let units = ddi::hold_units(end - start);
    let grace = Difficulty::Normal.seconds(if md.roll {
        ddi::ROLL_GRACE_UNITS
    } else {
        ddi::HOLD_GRACE_UNITS
    });
    let accepted = MINE_RESPONSE_V1
        .table
        .response(MINE_RESPONSE_V1.table.avoidance(level))
        .hit;
    let next = l.heads.get(idx + 1).copied().unwrap_or(f64::INFINITY);
    let stop = if md.roll {
        end.min(next - HOLD.roll_stop)
    } else {
        (end + HOLD.hold_after).min(next - HOLD.release_before)
    };
    let mut alive = 1.0;
    let mut j = row(g, start) + 1;
    let mut hazard = l.mines.partition_point(|t| *t <= start);
    let mut n = 1;
    let mut expected = 0.;
    let mut accidental = 0.;
    for k in 1..=units {
        let check = (start + f64::from(k)).min(end);
        let before = check - grace;
        if stop < before {
            break;
        }
        if start + 0.012 >= before {
            accidental += 1.;
        }
        while j < g.rows.len() && g.rows[j].0 <= before && g.rows[j].0 < end {
            if drops.masks[j] & (1 << lane) == 0 {
                alive *= 1. - drops.per_row[j][lane];
            }
            j += 1;
        }
        while start + f64::from(n) <= before && start + f64::from(n) <= end {
            alive *= 1. - drops.floor;
            n += 1;
        }
        while hazard < l.mines.len() && l.mines[hazard] <= before && l.mines[hazard] < end {
            // For hold bodies, lifting and regrabbing before the judge's grace normally suffices.
            // If the player remains on the mine a lane lock interrupts the hold.
            if !md.roll {
                alive *= (1. - accepted).powf(mult[hazard] as f64);
            }
            hazard += 1;
        }
        expected += alive;
    }
    (expected, accidental)
}

#[derive(Default)]
struct Counts {
    heads: f64,
    timing_loss: f64,
    mine_hits: [f64; 5], // indexed by Source
    hold_units: f64,
}
impl Counts {
    fn accumulate(&mut self, lane: Self, weight: f64) {
        self.heads += weight * lane.heads;
        self.timing_loss += weight * lane.timing_loss;
        self.hold_units += weight * lane.hold_units;
        for (total, count) in self.mine_hits.iter_mut().zip(lane.mine_hits) {
            *total += weight * count;
        }
    }
}
fn lane(
    g: &Geometry,
    j: usize,
    ls: &[Law],
    meta: &[Meta],
    events: &[Event],
    worth: &[(f64, f64)],
    plans: &[Option<PurePlan>],
) -> Option<Counts> {
    let l = &g.lanes[j];
    let hs = &l.heads;
    let ms = &l.mines;
    let tiers = ddi::TIER_UNITS.map(|v| Difficulty::Normal.seconds(v));
    let win = tiers[5];
    let mw = ddi::MINE_WINDOW;
    let mut out = Counts::default();
    let mut states = State::from([((0, 0), 1.)]);
    for ev in events {
        let t = ev.at;
        let half = match ev.timing {
            Timing::Motor(_) => BOUND,
            Timing::Uniform => HOLD.mine_place,
            Timing::Fixed => 0.,
        };
        let left = t - half;
        let right = t + half;
        if let Timing::Motor(i) = ev.timing
            && matches!(ev.source, Source::Primary)
            && let Some(plan) = &plans[ev.head]
            && states
                .keys()
                .all(|&(_, last)| last == 0 || ms[last - 1] + ddi::LANE_LOCK < left)
        {
            states = apply_pure(hs, ev, plan, &ls[i], &states, worth, &mut out);
            continue;
        }
        let a = hs.partition_point(|v| *v < left - win);
        let b = hs.partition_point(|v| *v <= right + win);
        let ml = ms.partition_point(|v| *v < left - mw);
        let mr = ms.partition_point(|v| *v <= right + mw);
        let primary = matches!(ev.source, Source::Primary);
        let mut reduced = State::new();
        for ((k, last), p) in states {
            let last = if last > 0 && ms[last - 1] + ddi::LANE_LOCK >= left {
                last
            } else {
                0
            };
            credit(&mut reduced, (k.max(a), last), p);
        }
        let mut cuts = vec![left, right];
        let mut add = |s| {
            if s > left && s < right {
                cuts.push(s);
            }
        };
        let crowded = b - a > 16 || mr - ml > 16;
        if crowded {
            for q in 1..32 {
                add(left + (right - left) * q as f64 / 32.);
            }
            // Keep the dominant head's real judgment breakpoints.
            if primary {
                for w in tiers {
                    for at in [t - w, t + w] {
                        add(at);
                    }
                }
            }
        } else {
            for &h in &hs[a..b] {
                for w in tiers {
                    add(h - w);
                    add(h + w);
                }
            }
            for &m in &ms[ml..mr] {
                add(m - mw);
                add(m + mw);
                add(m);
                add(m - 0.012);
                add(m - HOLD.mine_clear);
                add(m + HOLD.mine_clear);
                for &h in &hs[a..b] {
                    add((m + h) / 2.);
                }
            }
            if matches!(ev.source, Source::Deliberate) {
                // Subdivide overlap probabilities on the 90-ms placement window.
                for q in 1..12 {
                    add(left + (right - left) * q as f64 / 12.);
                }
            }
            for &(_, last) in reduced.keys() {
                if last > 0 {
                    add(ms[last - 1] + ddi::LANE_LOCK);
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-11);
        let mut next = State::new();
        let omit = match ev.timing {
            Timing::Motor(k) => ls[k].omit,
            _ => 0.,
        };
        let emit = ev.emit * (1. - omit);
        for (&k, &p) in &reduced {
            credit(&mut next, k, p * (1. - emit));
        }
        let mut act = |at: f64, mass: f64| {
            if mass <= 0. {
                return;
            }
            let exp = a + hs[a..b].partition_point(|v| *v < at - win);
            let m0 = ml + ms[ml..mr].partition_point(|v| *v < at - mw);
            let free = if matches!(ev.source, Source::Deliberate) {
                free(l, meta, ls, g, at, t)
            } else if matches!(ev.source, Source::Roll)
                && (at > meta[ev.head].end
                    || ms[ml..mr].iter().any(|&v| (v - at).abs() < HOLD.mine_clear))
            {
                0.
            } else {
                1.
            };
            for (&(k, last), &v) in &reduced {
                let h = k.max(exp);
                let p = v * mass * emit;
                credit(&mut next, (k, last), p * (1. - free));
                let p = p * free;
                if last > 0 && at < ms[last - 1] + ddi::LANE_LOCK {
                    credit(&mut next, (h, last), p);
                    continue;
                }
                if matches!(ev.source, Source::Held) {
                    if ev.mine >= last {
                        out.mine_hits[Source::Held as usize] += p;
                        credit(&mut next, (h, ev.mine + 1), p);
                    } else {
                        credit(&mut next, (h, last), p);
                    }
                    continue;
                }
                let m = last.max(m0);
                let tap = h < b && (hs[h] - at).abs() <= win;
                let mine = m < mr && (ms[m] - at).abs() <= mw;
                let win_m = mine && (!tap || (ms[m] - at).abs() < (hs[h] - at).abs());
                if win_m {
                    out.mine_hits[ev.source as usize] += p;
                    credit(&mut next, (h, m + 1), p);
                    continue;
                }
                // An auxiliary event is modeled only for its mine interaction; readouts for
                // unrelated successful secondary taps/rolls are outside the primary control.
                let landed = tap && (primary || mine);
                let nh = if landed {
                    let d = (hs[h] - at).abs();
                    let ix = tiers[1..].partition_point(|&w| d > w).min(4);
                    out.heads += p;
                    let (own, short) = worth[h];
                    out.hold_units += p * if primary && ev.head == h { own } else { short };
                    out.timing_loss += p * (50 - ddi::TAP_POINTS[ix]) as f64;
                    h + 1
                } else {
                    h
                };
                let cross = ms.partition_point(|v| *v < at).max(last);
                if ev.short && cross < ms.len() && ms[cross] < at + 0.012 && ms[cross] >= at {
                    out.mine_hits[Source::Held as usize] += p;
                    credit(&mut next, (nh, cross + 1), p);
                } else {
                    credit(&mut next, (nh, last), p);
                }
            }
        };
        match ev.timing {
            Timing::Fixed => act(t, 1.),
            Timing::Uniform => {
                for w in cuts.windows(2) {
                    act((w[0] + w[1]) * 0.5, (w[1] - w[0]) / (2. * half));
                }
            }
            Timing::Motor(k) => {
                let law = &ls[k];
                for w in cuts.windows(2) {
                    act(
                        (w[0] + w[1]) * 0.5,
                        law.raw_edges_cdf(w[1] - t - law.shift)
                            - law.raw_edges_cdf(w[0] - t - law.shift),
                    );
                }
                for (x, p) in law.atoms() {
                    act(t + x, p);
                }
            }
        }
        if crowded && next.len() > 20 {
            // Quantile resampling retains the joint distribution over pending head and
            // prior lock. A 20-point support bounds work inside impossible head bursts.
            let mut bounded = State::new();
            let mut sum = 0.;
            let mut n = 0;
            for (&key, &p) in &next {
                sum += p;
                while n < 20 && (n as f64 + 0.5) / 20. <= sum {
                    credit(&mut bounded, key, 0.05);
                    n += 1;
                }
            }
            next = bounded;
        }
        let sum = next.values().sum::<f64>();
        if !sum.is_finite() || (sum - 1.).abs() >= 2e-7 {
            return None;
        }
        next.retain(|_, p| *p > 1e-11);
        let sum = next.values().sum::<f64>();
        for p in next.values_mut() {
            *p /= sum;
        }
        states = next;
    }
    Some(out)
}
fn estimate(
    g: &Geometry,
    level: usize,
    weak: bool,
    meta: &[Vec<Meta>; 4],
    counts: &[Vec<usize>; 4],
    plans: &[Vec<Option<PurePlan>>; 4],
    grids: &mut TimingGrids,
) -> Option<Counts> {
    let mut masks = vec![0u8; g.rows.len()];
    for (j, lane) in g.lanes.iter().enumerate() {
        for &t in &lane.heads {
            masks[row(g, t)] |= 1 << j;
        }
    }
    let a = &CPU_LEVELS_V2.levels[level];
    let m = a.model;
    let strength = if weak { 0.75_f64 } else { 1. };
    let mut sk = [1.; 7];
    sk[6] = strength;
    let sk = Skills::new(sk).unwrap();
    let floor = a.floor * HOLD.drop_share / strength.sqrt();
    let drop = g
        .rows
        .iter()
        .zip(&masks)
        .map(|((_, d), &mask)| {
            let (_, _, load) = row_response(a, &sk, d, m);
            std::array::from_fn(|j| {
                let hand = if j < 2 { 3 } else { 12 };
                let load = load * if mask & hand != 0 { HOLD.same_hand } else { 1. };
                (m.lapse_over(a.floor, (load - m.onset).max(0.)) * HOLD.drop_share
                    / strength.sqrt())
                .min(m.most_lapse)
            })
        })
        .collect::<Vec<_>>();
    let drops = HoldDrops {
        masks,
        per_row: drop,
        floor,
    };
    let mut stats = Counts::default();
    let worth: Vec<Vec<_>> = meta
        .iter()
        .enumerate()
        .map(|(j, hs)| {
            hs.iter()
                .enumerate()
                .map(|(i, &md)| body_worth(g, j, i, md, &drops, level, &counts[j]))
                .collect()
        })
        .collect();
    let z3 = 3_f64.sqrt();
    let nodes = [(-z3, 1. / 6.), (0., 2. / 3.), (z3, 1. / 6.)];
    for (z, p) in nodes {
        for (v, q) in nodes {
            let laws = laws(g, level, weak, z, v, grids);
            for (j, md) in meta.iter().enumerate() {
                let ev = events(g, j, level, &laws, md, &drops, &counts[j]);
                let res = lane(g, j, &laws, md, &ev, &worth[j], &plans[j])?;
                stats.accumulate(res, p * q);
            }
        }
    }
    Some(stats)
}

// Bound the representation of same-lane mine walls. Preserve sparse mines exactly;
// pool only bins under pressure (>8 in 90 ms). Keep multiplicity for deliberate
// and held contact probability and for hold-body survival.
fn compress_wall(ms: &mut Vec<f64>) -> Vec<usize> {
    let mut reps = Vec::with_capacity(ms.len());
    let mut counts = Vec::with_capacity(ms.len());
    let mut i = 0;
    while i < ms.len() {
        let end = if ms.get(i + 8).is_some_and(|&t| t - ms[i] < 0.09) {
            i + ms[i..].partition_point(|&t| t <= ms[i] + 0.0225)
        } else {
            i + 1
        };
        reps.push((ms[i] + ms[end - 1]) * 0.5);
        counts.push(end - i);
        i = end;
    }
    *ms = reps;
    counts
}
pub(super) fn display_cost(chart: &Chart, decimal: f64) -> Option<f64> {
    let mut g = Geometry {
        lanes: Default::default(),
        rows: Vec::new(),
    };
    let mut meta: [Vec<Meta>; 4] = Default::default();
    for row in &chart.rows {
        if row.demand.count > 0 {
            g.rows.push((row.time, row.demand));
        }
        let [taps, holds, rolls, lifts, mines] = row.masks;
        for (j, lane) in g.lanes.iter_mut().enumerate() {
            let bit = 1 << j;
            if mines & bit != 0 {
                lane.mines.push(row.time);
            }
            if (taps | holds | rolls | lifts) & bit != 0 {
                lane.heads.push(row.time);
                meta[j].push(Meta {
                    end: row.time + row.durations[j],
                    roll: rolls & bit != 0,
                });
            }
        }
    }
    let counts = std::array::from_fn(|j| compress_wall(&mut g.lanes[j].mines));
    let plans = pure_plans(&g);
    let (lo, hi, f) = super::radar::matched_levels(decimal);
    let mut total = [0.; 4];
    for (level, weight) in [(lo, 1. - f), (hi, f)] {
        if weight <= 0. {
            continue;
        }
        let mut grids = TimingGrids::new();
        let balanced = estimate(&g, level, false, &meta, &counts, &plans, &mut grids)?;
        let weak = estimate(&g, level, true, &meta, &counts, &plans, &mut grids)?;
        // Raw Accuracy is expressed in percentage points. A perfect head earns
        // 50 points, a miss costs 120 and a mine costs 50 under this teacher.
        let delta = [
            2. * (weak.timing_loss - balanced.timing_loss),
            340. * (balanced.heads - weak.heads),
            100. * (weak.mine_hits.iter().sum::<f64>() - balanced.mine_hits.iter().sum::<f64>()),
            100. * (balanced.hold_units - weak.hold_units),
        ];
        for (s, v) in total.iter_mut().zip(delta) {
            *s += weight * v / (chart.heads + chart.units);
        }
    }
    let cost = calibrated_cost(total, decimal);
    cost.is_finite().then_some(cost)
}
struct Head {
    center: [f64; 5],
    scale: [f64; 5],
    projection: [[f64; 12]; 5],
    offset: [f64; 12],
    beta: [f64; 18],
}

// Apply the three frozen bounded residual heads, averaging signed costs before display rounding.
fn calibrated_cost(parts: [f64; 4], scalar: f64) -> f64 {
    let x = [
        parts[0].asinh(),
        parts[1].asinh(),
        parts[2].asinh(),
        parts[3].asinh(),
        scalar / 19.0,
    ];
    let direct = parts.iter().sum::<f64>();
    weights::HEADS
        .iter()
        .map(|head| {
            let mut hidden = head.offset;
            let mut residual = head.beta[0];
            for (i, value) in x.iter().enumerate() {
                let z = (value - head.center[i]) / head.scale[i];
                residual += z * head.beta[i + 1];
                for (t, w) in hidden.iter_mut().zip(head.projection[i]) {
                    *t += z * w;
                }
            }
            for (t, b) in hidden.iter().zip(&head.beta[6..]) {
                residual += t.tanh() * b;
            }
            direct + 0.25 * (residual / 0.25).tanh()
        })
        .sum::<f64>()
        / 3.
}
