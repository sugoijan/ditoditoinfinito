//! A CPU playing the chart a person is given (`cpu-play-v1`,
//! `docs/plans/cpu-player.md`, "What a CPU plays"): its taps, its holds
//! kept down to their ends, its rolls re-tapped, its lifts released and
//! its mines avoided, all judged by the production judge under the
//! stage's rules, so that its life, its judgments and its score are on
//! the chart's own scale. Fakes are not played; warps, stops and delays
//! are the judge's (it presses on the judged timeline).
//!
//! The earlier play, `cpu-play-v0` ([`CpuPlay::TapsV0`]), is the chart's
//! tap-only copy (`super::project`): kept by name for the prepared
//! profiles, the chart-relative levels (`cpu-levels-v0`) and comparison.
//!
//! How each note is played (`hold-model-v1`, [`HOLD_MODEL_V1`],
//! PROVISIONAL):
//!
//! - **Every note's timing** is the absolute model's
//!   (`super::absolute`): the motor's three draws a note, in rows of the
//!   chart's taps, hold and roll heads and lifts, each row's spread and
//!   chance of being left out from its demand. A row played while holds or
//!   rolls are down asks more ([`RowDemand::held`],
//!   `AbsoluteModel::held_load`).
//! - **A hold**: its head pressed as a tap is, the key kept down to the
//!   hold's end and let go just after it. It may be **let go early**: at
//!   each row while it lasts (in other lanes) with a chance on the
//!   level's lapse curve over that row's load, the load counted
//!   [`HoldModel::same_hand`] times where the row asks something of the
//!   held lane's hand (lanes 0 and 1, 2 and 3), and at each whole second
//!   of it with the level's floor; Stamina and Technical divide those
//!   chances (by the square root of their product). Let go, it stays up:
//!   the judge drops it (DDI's Off).
//! - **A roll**: its head pressed, then re-tapped every
//!   [`HoldModel::roll_interval`] (well inside every ruleset's roll
//!   window, so one tap left out is survived and two are not) with the
//!   timing law of the row before each tap, each tap left out with that
//!   row's lapse; let go early as a hold is (its taps stop).
//! - **A lift**: the key pressed [`HoldModel::lift_lead`] before its
//!   release, the release on the lift's time with the timing law.
//! - **The chart's own mines**, as a battle's shocks are
//!   (`super::MINE_RESPONSE_V1`, Evasion): one with no note of its lane
//!   within half a second costs nothing; one beside a note of its lane is
//!   stepped on (a press in its window) with the level's chance bent by
//!   Evasion where it is hard to get around; and a press of the timing
//!   law the judge finds nearer a mine than its note steps on it, as a
//!   person's would. One **inside a hold** in the held lane is kept held
//!   over (stepped on) with that same chance (as beside a note), else the
//!   key is let go [`HoldModel::mine_before`] before it and taken again
//!   [`HoldModel::mine_after`] after it, which the hold survives.
//! - **The motor**: a lane is busy while a hold or roll is in it; a tap's
//!   key is held 12 ms; no two presses of one lane overlap (a press due
//!   while its lane is busy waits for it). Four lanes are four keys, so a
//!   chart never asks for more than four at once.
//!
//! Every draw beyond the motor's is keyed by the seed, the note (its tick
//! and lane) and what it is for (`super::mines::draw`), so the same chart,
//! level, seed, skills and shocks give the same presses at any frame rate,
//! natively and in wasm.

use super::absolute::{ABSOLUTE_V1, Ability, RowDemand, law, loads, respond, row_demands};
use super::mines::{MINE_RESPONSE_V1, MineResponse, Purpose, draw};
use super::{MAX_AUTOMATIC_ROWS, OMITTED, Press, Random, Row, Skills, basis, offset};
use crate::{Judge, presets};
use ddi_chart::{NoteKind, Song, Tick};
use std::collections::BTreeMap;

/// What a CPU plays of a chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CpuPlay {
    /// `cpu-play-v0`: the chart's tap-only copy (holds and rolls as their
    /// heads, mines, lifts and tails left out; `super::project`).
    TapsV0,
    /// `cpu-play-v1`: the chart itself, as a person is given it.
    FullV1,
}

impl CpuPlay {
    pub fn id(self) -> &'static str {
        match self {
            CpuPlay::TapsV0 => "cpu-play-v0",
            CpuPlay::FullV1 => "cpu-play-v1",
        }
    }

    /// What a CPU at `strength` plays: an absolute level the chart itself
    /// ([`CPU_PLAY`]); a chart-relative one (`cpu-levels-v0`, and every
    /// prepared profile) the tap-only copy, exactly as before.
    pub fn of(strength: &super::Strength) -> CpuPlay {
        match strength {
            super::Strength::Absolute(_) => CPU_PLAY,
            super::Strength::Relative(_) => CpuPlay::TapsV0,
        }
    }
}

/// What the CPUs at absolute levels play (2026-11-14, the maintainer:
/// "CPUs should play the full charts").
pub const CPU_PLAY: CpuPlay = CpuPlay::TapsV0;

/// How a CPU plays what a tap-only copy leaves out (`hold-model-v1`,
/// PROVISIONAL: working values, measured over charts, never against
/// people).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoldModel {
    pub id: &'static str,
    /// Seconds after a hold's end its key is let go.
    pub hold_after: f64,
    /// A hold's or roll's key let go at least this long before the lane's
    /// next note (seconds).
    pub release_before: f64,
    /// A row asking something of the held lane's hand counts this many
    /// times its load for letting go.
    pub same_hand: f64,
    /// The chance of letting go at a row, as a share of the lapse the
    /// level's curve gives that row's load.
    pub drop_share: f64,
    /// Seconds between a roll's taps, and how long each is held.
    pub roll_interval: f64,
    pub roll_tap: f64,
    /// No roll tap later than this before the lane's next note (the
    /// autoplayer's `ROLL_TAP_STOP`).
    pub roll_stop: f64,
    /// Seconds before a lift's release its key goes down.
    pub lift_lead: f64,
    /// A mine inside a hold: the key let go this long before it and taken
    /// again this long after it.
    pub mine_before: f64,
    pub mine_after: f64,
    /// No roll tap this close to a mine of its lane (beyond every mine
    /// window).
    pub mine_clear: f64,
    /// A mine stepped on: the press this far from its time at most,
    /// either way (seconds).
    pub mine_place: f64,
}

/// `hold-model-v1` (PROVISIONAL).
pub const HOLD_MODEL_V1: HoldModel = HoldModel {
    id: "hold-model-v1",
    hold_after: 0.012,
    release_before: 0.05,
    same_hand: 1.25,
    drop_share: 0.5,
    roll_interval: 0.12,
    roll_tap: 0.012,
    roll_stop: 0.23,
    lift_lead: 0.08,
    mine_before: 0.03,
    mine_after: 0.1,
    mine_clear: 0.09,
    mine_place: 0.045,
};

/// What a note of the full chart is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Tap,
    Hold { end: f64 },
    Roll { end: f64 },
    Lift,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FullNote {
    /// Its row (an index into [`FullPlan::rows`]).
    row: u32,
    lane: u8,
    tick: i64,
    kind: Kind,
    /// The judged second of the lane's next tap-like note (infinite
    /// where none).
    next: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ChartMine {
    seconds: f64,
    lane: u8,
    tick: i64,
}

/// The full chart as a CPU plays it: its rows of tap-like notes (taps,
/// hold and roll heads, lifts) in judged time order with their demands,
/// each note and what it is, and the chart's mines.
#[derive(Clone, Debug, PartialEq)]
pub struct FullPlan {
    rows: Vec<Row>,
    demands: Vec<RowDemand>,
    notes: Vec<FullNote>,
    mines: Vec<ChartMine>,
}

impl FullPlan {
    /// Chart `index` of `song` as a person is given it; refused where a
    /// CPU cannot play it (not one-player four lanes, no tap-like notes,
    /// over the limits).
    pub fn new(song: &Song, index: usize) -> Result<FullPlan, String> {
        let chart = song.charts.get(index).ok_or("CPU chart missing")?;
        if !song
            .layout_of(chart)
            .is_some_and(|layout| layout.players == 1 && layout.lanes.len() == 4)
        {
            return Err("CPU supports single-player keyboard 4K charts only".into());
        }
        if chart.notes.len() > MAX_AUTOMATIC_ROWS * 4 {
            return Err("CPU chart exceeds the note limit".into());
        }
        let judge = Judge::new(&chart.notes, chart.timing(song), presets::itg().judge);
        let mut by_tick: BTreeMap<Tick, (Row, [Option<Kind>; 4])> = BTreeMap::new();
        let mut mines = Vec::new();
        for n in judge.notes().iter().filter(|n| n.judgeable) {
            let kind = match n.note.kind {
                NoteKind::Tap => Kind::Tap,
                NoteKind::HoldHead { .. } => Kind::Hold {
                    end: n.end_seconds,
                },
                NoteKind::RollHead { .. } => Kind::Roll {
                    end: n.end_seconds,
                },
                NoteKind::Lift => Kind::Lift,
                NoteKind::Mine | NoteKind::Shock => {
                    if n.note.lane < 4 && n.seconds.is_finite() && n.seconds.abs() <= 86400. {
                        mines.push(ChartMine {
                            seconds: n.seconds,
                            lane: n.note.lane,
                            tick: n.note.tick.0,
                        });
                    }
                    continue;
                }
                _ => continue,
            };
            if n.note.lane >= 4
                || !n.seconds.is_finite()
                || n.seconds.abs() > 86400.
                || !n.end_seconds.is_finite()
            {
                return Err("CPU expects finite 4K notes within one day".into());
            }
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
        if by_tick.is_empty() || by_tick.len() > MAX_AUTOMATIC_ROWS {
            return Err(format!(
                "CPU needs 1–{MAX_AUTOMATIC_ROWS} playable tap rows"
            ));
        }
        let mut rows: Vec<(Tick, Row, [Option<Kind>; 4])> =
            by_tick.into_iter().map(|(t, (r, k))| (t, r, k)).collect();
        // Judged seconds, as the tap-only copy's rows are ordered.
        rows.sort_by(|a, b| a.1.seconds.total_cmp(&b.1.seconds));
        let mut notes = Vec::new();
        for (i, (tick, _, kinds)) in rows.iter().enumerate() {
            for (lane, kind) in kinds.iter().enumerate() {
                if let Some(kind) = *kind {
                    notes.push(FullNote {
                        row: i as u32,
                        lane: lane as u8,
                        tick: tick.0,
                        kind,
                        next: f64::INFINITY,
                    });
                }
            }
        }
        let rows: Vec<Row> = rows.into_iter().map(|(_, r, _)| r).collect();
        let mut next = [f64::INFINITY; 4];
        for n in notes.iter_mut().rev() {
            let lane = usize::from(n.lane);
            n.next = next[lane];
            next[lane] = rows[n.row as usize].seconds;
        }
        mines.sort_by(|a, b| a.seconds.total_cmp(&b.seconds).then(a.lane.cmp(&b.lane)));
        let mut demands = row_demands(&rows, &ABSOLUTE_V1);
        // The lanes held while each row is played: holds and rolls begun
        // at an earlier row and lasting past it, in other lanes.
        let mut active: Vec<(f64, u8)> = Vec::new();
        let mut k = 0;
        for (i, row) in rows.iter().enumerate() {
            active.retain(|&(end, _)| end > row.seconds);
            let held = active
                .iter()
                .filter(|&&(_, lane)| row.mask & (1 << lane) == 0)
                .map(|&(_, lane)| 1u8 << lane)
                .fold(0u8, |m, b| m | b);
            demands[i].held = held.count_ones() as u8;
            while k < notes.len() && notes[k].row as usize == i {
                if let Kind::Hold { end } | Kind::Roll { end } = notes[k].kind {
                    active.push((end, notes[k].lane));
                }
                k += 1;
            }
        }
        Ok(FullPlan {
            rows,
            demands,
            notes,
            mines,
        })
    }

    /// Its rows' demands in real units (with what is held at each).
    pub fn demands(&self) -> &[RowDemand] {
        &self.demands
    }

    /// Its holds and rolls, and the chart's mines.
    pub fn counts(&self) -> (usize, usize, usize) {
        let holds = self
            .notes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Hold { .. }))
            .count();
        let rolls = self
            .notes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Roll { .. }))
            .count();
        (holds, rolls, self.mines.len())
    }
}

/// Check a full play's presses: bounded, legal, in time order, every key
/// held at least 12 ms, no two presses of one lane overlapping.
pub fn validate_full_script(presses: &[Press]) -> Result<(), String> {
    if presses.len() > MAX_AUTOMATIC_ROWS * 64 {
        return Err("CPU script exceeds the note limit".into());
    }
    let mut last = f64::NEG_INFINITY;
    let mut released = [f64::NEG_INFINITY; 4];
    for p in presses {
        if p.lane >= 4
            || !p.press.is_finite()
            || !p.release.is_finite()
            || p.press.abs() > 86401.
            || p.release.abs() > 86401.
            || p.release - p.press < 0.012 - 1e-8
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

/// What one row is to the CPU: its timing law's key, its lapse and its
/// load for letting a hold go.
struct RowPlay {
    tenths: u32,
    lapse: f64,
    load: f64,
}

/// The presses of a CPU of `a` with `skills` and `seed` playing `plan`,
/// its mines' response the level's `mines` (bent by its Evasion).
pub(super) fn script(
    plan: &FullPlan,
    a: Ability,
    seed: u64,
    skills: Skills,
    mines: MineResponse,
) -> Result<Vec<Press>, String> {
    a.validate()?;
    let m = &ABSOLUTE_V1;
    let h = &HOLD_MODEL_V1;
    let v = skills.values();
    let endurance = (v[3] * v[6]).sqrt();
    let evasion = skills.evasion();
    let over = |x: f64| if x > 0.0 { x } else { 0.0 };
    let rows: Vec<RowPlay> = plan
        .demands
        .iter()
        .map(|d| {
            let (spread, lapse) = respond(&a, &skills, d, m);
            let (load, lanes) = loads(&a, &skills, d, m);
            RowPlay {
                tenths: (spread * 10.0).round().max(10.0) as u32,
                lapse,
                load: load.max(lanes),
            }
        })
        .collect();
    let mut laws: BTreeMap<u32, [f64; 91]> = BTreeMap::new();
    let mut law_of = |tenths: u32| *laws.entry(tenths).or_insert_with(|| law(tenths, m));
    // Each lane's mines in time order.
    let mut lane_mines: [Vec<usize>; 4] = Default::default();
    for (i, mine) in plan.mines.iter().enumerate() {
        lane_mines[usize::from(mine.lane)].push(i);
    }
    // The notes (seconds, lane) a mine's place is read against.
    let context: Vec<(f64, u8)> = plan
        .notes
        .iter()
        .map(|n| (plan.rows[n.row as usize].seconds, n.lane))
        .collect();
    let response_at = |at: f64, lane: u8, beside: bool| {
        let mut c = MINE_RESPONSE_V1.context(&context, at, lane);
        c.beside |= beside;
        (c.beside, MINE_RESPONSE_V1.response(mines, evasion, c))
    };
    let hand = |lane: u8| if lane < 2 { 0b0011u8 } else { 0b1100 };
    // The chance a hold in `lane` is let go at row `j`.
    let drop_chance = |j: usize, lane: u8| {
        let load = rows[j].load
            * if plan.rows[j].mask & hand(lane) != 0 {
                h.same_hand
            } else {
                1.0
            };
        let x = over(load - m.onset);
        ((a.floor + m.lapse_linear * x + m.lapse_square * x * x) * h.drop_share / endurance)
            .min(m.most_lapse)
    };
    let mut rng = Random(seed);
    let mut out: Vec<Press> = Vec::with_capacity(plan.notes.len() + 16);
    let mut free = [f64::NEG_INFINITY; 4];
    // Mines a hold's play already decided.
    let mut decided = vec![false; plan.mines.len()];
    let push = |out: &mut Vec<Press>, free: &mut [f64; 4], press: f64, release: f64, lane: u8| {
        let release = release.max(press + 0.012);
        out.push(Press {
            press,
            release,
            lane,
        });
        free[usize::from(lane)] = release;
    };
    for note in &plan.notes {
        let i = note.row as usize;
        let row = &rows[i];
        let t = plan.rows[i].seconds;
        let lane = note.lane;
        let l = usize::from(lane);
        let u = [rng.unit(), rng.unit(), rng.unit()];
        let omit = OMITTED + row.lapse * (1.0 - OMITTED);
        if u[2] < omit {
            continue;
        }
        let q = law_of(row.tenths);
        let off = offset(&basis::EDGES, &q, u[0], u[1]);
        let earliest = free[l] + 0.0001;
        match note.kind {
            Kind::Tap => {
                let press = (t + off).max(earliest);
                push(&mut out, &mut free, press, press + 0.012, lane);
            }
            Kind::Lift => {
                let up = t + off;
                let press = (up - h.lift_lead).max(earliest);
                push(&mut out, &mut free, press, up, lane);
            }
            Kind::Hold { end } | Kind::Roll { end } => {
                let press = (t + off).max(earliest);
                // Let go early: the first row while it lasts, or the first
                // whole second of it, whose draw says so.
                let mut dropped: Option<f64> = None;
                let mut j = i + 1;
                while j < plan.rows.len() && plan.rows[j].seconds < end {
                    if plan.rows[j].mask & (1 << lane) == 0
                        && draw(seed, note.tick, lane, Purpose::Drop, j as u64)
                            < drop_chance(j, lane)
                    {
                        dropped = Some(plan.rows[j].seconds);
                        break;
                    }
                    j += 1;
                }
                let mut k = 1u64;
                while t + k as f64 <= end && dropped.is_none_or(|d| t + (k as f64) < d) {
                    if draw(seed, note.tick, lane, Purpose::DropFloor, k)
                        < a.floor * h.drop_share / endurance
                    {
                        dropped = Some(t + k as f64);
                        break;
                    }
                    k += 1;
                }
                // Up before the lane's next note, and before a mine just
                // past its end.
                let mut stop = dropped.unwrap_or(end + h.hold_after).min(note.next - h.release_before);
                let inside: Vec<usize> = lane_mines[l]
                    .iter()
                    .copied()
                    .filter(|&k| plan.mines[k].seconds > t && plan.mines[k].seconds <= stop)
                    .collect();
                if let Kind::Hold { .. } = note.kind {
                    let mut from = press;
                    for k in inside {
                        let at = plan.mines[k].seconds;
                        decided[k] = true;
                        if at >= end {
                            // Past its end: up before it.
                            stop = stop.min(at - 0.002);
                            break;
                        }
                        if at - h.mine_before < from + 0.012 {
                            // Too soon after the head to let go: held over.
                            continue;
                        }
                        let (_, r) = response_at(at, lane, true);
                        if draw(seed, plan.mines[k].tick, lane, Purpose::HoldMine, 0) < r.hit {
                            continue;
                        }
                        push(&mut out, &mut free, from, at - h.mine_before, lane);
                        from = at + h.mine_after;
                        if from + 0.012 > stop {
                            stop = f64::NEG_INFINITY;
                            break;
                        }
                    }
                    if stop.is_finite() {
                        push(&mut out, &mut free, from, stop, lane);
                    }
                } else {
                    for &k in &inside {
                        decided[k] = true;
                    }
                    push(&mut out, &mut free, press, press + h.roll_tap, lane);
                    let last = end.min(note.next - h.roll_stop).min(dropped.unwrap_or(f64::INFINITY));
                    let clear = |x: f64| {
                        !lane_mines[l]
                            .iter()
                            .any(|&k| (plan.mines[k].seconds - x).abs() < h.mine_clear)
                    };
                    // On a grid from the head, and either side of a mine
                    // of its lane (none near one).
                    let mut taps: Vec<f64> = std::iter::successors(Some(t + h.roll_interval), |k| {
                        Some(k + h.roll_interval)
                    })
                    .take_while(|&k| k < last)
                    .filter(|&k| clear(k))
                    .collect();
                    for &k in &lane_mines[l] {
                        let at = plan.mines[k].seconds;
                        if at > t && at < last {
                            for x in [at - 0.1, at + 0.1] {
                                if x > t && x < last && clear(x) {
                                    taps.push(x);
                                }
                            }
                        }
                    }
                    taps.sort_by(f64::total_cmp);
                    taps.dedup_by(|b, a| *b - *a < 0.06);
                    let mut j = i;
                    for (n, nominal) in taps.into_iter().enumerate() {
                        while j + 1 < plan.rows.len() && plan.rows[j + 1].seconds <= nominal {
                            j += 1;
                        }
                        let n = n as u64;
                        if draw(seed, note.tick, lane, Purpose::RollSlip, n) < rows[j].lapse {
                            continue;
                        }
                        let q = law_of(rows[j].tenths);
                        let u0 = draw(seed, note.tick, lane, Purpose::RollTap, 2 * n);
                        let u1 = draw(seed, note.tick, lane, Purpose::RollTap, 2 * n + 1);
                        let tap = (nominal + offset(&basis::EDGES, &q, u0, u1)).max(free[l] + 0.0001);
                        if tap > end || !clear(tap) {
                            continue;
                        }
                        push(&mut out, &mut free, tap, tap + h.roll_tap, lane);
                    }
                }
            }
        }
    }
    // The chart's mines a hold did not decide: stepped on beside a note
    // of their lane with the level's chance.
    let mut extra = Vec::new();
    for (k, mine) in plan.mines.iter().enumerate() {
        if decided[k] {
            continue;
        }
        let (beside, r) = response_at(mine.seconds, mine.lane, false);
        if !beside || draw(seed, mine.tick, mine.lane, Purpose::ChartMine, 0) >= r.hit {
            continue;
        }
        let u = draw(seed, mine.tick, mine.lane, Purpose::ChartMinePlace, 0);
        let press = mine.seconds + (2.0 * u - 1.0) * h.mine_place;
        extra.push(Press {
            press,
            release: press + 0.012,
            lane: mine.lane,
        });
    }
    out.sort_by(|a, b| a.press.total_cmp(&b.press).then(a.lane.cmp(&b.lane)));
    // A step on a mine where its lane is free alone.
    for e in extra {
        let clashes = out.iter().any(|p| {
            p.lane == e.lane && p.press < e.release + 0.0001 && e.press < p.release + 0.0001
        });
        if !clashes {
            let i = out.partition_point(|p| (p.press, p.lane) <= (e.press, e.lane));
            out.insert(i, e);
        }
    }
    // Each lane's presses apart (a safety net: the clamps above keep them
    // so).
    let mut released = [f64::NEG_INFINITY; 4];
    out.retain(|p| {
        let r = &mut released[usize::from(p.lane)];
        if p.press < *r + 0.0001 {
            return false;
        }
        *r = p.release;
        true
    });
    validate_full_script(&out)?;
    Ok(out)
}

#[cfg(test)]
mod tests;
